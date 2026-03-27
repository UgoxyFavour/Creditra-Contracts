#![no_std]
#![allow(clippy::unused_unit)]

//! Creditra credit contract: credit lines, draw/repay, and risk parameter updates.

mod events;
mod types;

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol,
};

use events::{
    publish_credit_line_event, publish_drawn_event, publish_repayment_event,
    publish_risk_parameters_updated, CreditLineEvent, DrawnEvent, RepaymentEvent,
    RiskParametersUpdatedEvent,
};
use types::{ContractError, CreditLineData, CreditStatus, RateChangeConfig};

/// Maximum interest rate in basis points (100%).
const MAX_INTEREST_RATE_BPS: u32 = 10_000;

/// Maximum risk score (0-100 scale).
const MAX_RISK_SCORE: u32 = 100;

fn reentrancy_key(env: &Env) -> Symbol {
    Symbol::new(env, "reentrancy")
}

fn admin_key(env: &Env) -> Symbol {
    Symbol::new(env, "admin")
}

fn rate_cfg_key(env: &Env) -> Symbol {
    Symbol::new(env, "rate_cfg")
}

fn require_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&admin_key(env))
        .expect("admin not set")
}

fn require_admin_auth(env: &Env) -> Address {
    let admin = require_admin(env);
    admin.require_auth();
    admin
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    LiquidityToken,
    LiquiditySource,
}

fn set_reentrancy_guard(env: &Env) {
    let key = reentrancy_key(env);
    let current = env
        .storage()
        .instance()
        .get::<Symbol, bool>(&key)
        .unwrap_or(false);
    if current {
        env.panic_with_error(ContractError::Reentrancy);
    }
    env.storage().instance().set(&key, &true);
}

fn clear_reentrancy_guard(env: &Env) {
    env.storage().instance().set(&reentrancy_key(env), &false);
}

fn checked_add_i128(env: &Env, lhs: i128, rhs: i128) -> i128 {
    lhs.checked_add(rhs)
        .unwrap_or_else(|| env.panic_with_error(ContractError::Overflow))
}

fn checked_sub_i128(env: &Env, lhs: i128, rhs: i128) -> i128 {
    lhs.checked_sub(rhs)
        .unwrap_or_else(|| env.panic_with_error(ContractError::Overflow))
}

#[contract]
pub struct Credit;

#[contractimpl]
impl Credit {
    /// Initializes contract-level configuration.
    /// Sets admin and defaults liquidity source to the contract address.
    pub fn init(env: Env, admin: Address) -> () {
        env.storage().instance().set(&admin_key(&env), &admin);
        env.storage()
            .instance()
            .set(&DataKey::LiquiditySource, &env.current_contract_address());
        ()
    }

    /// Sets the token contract used for reserve/liquidity checks and draw transfers.
    /// Admin-only.
    pub fn set_liquidity_token(env: Env, token_address: Address) -> () {
        require_admin_auth(&env);
        env.storage()
            .instance()
            .set(&DataKey::LiquidityToken, &token_address);
        ()
    }

    /// Sets the address that provides liquidity for draw operations.
    /// Admin-only.
    pub fn set_liquidity_source(env: Env, reserve_address: Address) -> () {
        require_admin_auth(&env);
        env.storage()
            .instance()
            .set(&DataKey::LiquiditySource, &reserve_address);
        ()
    }

    /// Opens a new credit line for a borrower.
    pub fn open_credit_line(
        env: Env,
        borrower: Address,
        credit_limit: i128,
        interest_rate_bps: u32,
        risk_score: u32,
    ) {
        assert!(credit_limit > 0, "credit_limit must be greater than zero");
        assert!(
            interest_rate_bps <= MAX_INTEREST_RATE_BPS,
            "interest_rate_bps cannot exceed 10000 (100%)"
        );
        assert!(
            risk_score <= MAX_RISK_SCORE,
            "risk_score must be between 0 and 100"
        );

        if let Some(existing) = env
            .storage()
            .persistent()
            .get::<Address, CreditLineData>(&borrower)
        {
            assert!(
                existing.status != CreditStatus::Active,
                "borrower already has an active credit line"
            );
        }

        let credit_line = CreditLineData {
            borrower: borrower.clone(),
            credit_limit,
            utilized_amount: 0,
            interest_rate_bps,
            risk_score,
            status: CreditStatus::Active,
            last_rate_update_ts: 0,
        };

        env.storage().persistent().set(&borrower, &credit_line);

        publish_credit_line_event(
            &env,
            (symbol_short!("credit"), symbol_short!("opened")),
            CreditLineEvent {
                event_type: symbol_short!("opened"),
                borrower,
                status: CreditStatus::Active,
                credit_limit,
                interest_rate_bps,
                risk_score,
            },
        );
    }

    /// Draws credit by transferring liquidity tokens to the borrower.
    pub fn draw_credit(env: Env, borrower: Address, amount: i128) -> () {
        set_reentrancy_guard(&env);
        borrower.require_auth();

        if amount <= 0 {
            clear_reentrancy_guard(&env);
            panic!("amount must be positive");
        }

        let token_address: Option<Address> = env.storage().instance().get(&DataKey::LiquidityToken);
        let reserve_address: Address = env
            .storage()
            .instance()
            .get(&DataKey::LiquiditySource)
            .unwrap_or(env.current_contract_address());

        let mut credit_line: CreditLineData = env
            .storage()
            .persistent()
            .get(&borrower)
            .expect("Credit line not found");

        if credit_line.status == CreditStatus::Closed {
            clear_reentrancy_guard(&env);
            panic!("credit line is closed");
        }

        if credit_line.status != CreditStatus::Active {
            clear_reentrancy_guard(&env);
            panic!("Credit line not active");
        }

        let updated_utilized = checked_add_i128(&env, credit_line.utilized_amount, amount);

        if updated_utilized > credit_line.credit_limit {
            clear_reentrancy_guard(&env);
            panic!("exceeds credit limit");
        }

        if let Some(token_address) = token_address {
            let token_client = token::Client::new(&env, &token_address);
            let reserve_balance = token_client.balance(&reserve_address);
            if reserve_balance < amount {
                clear_reentrancy_guard(&env);
                panic!("Insufficient liquidity reserve for requested draw amount");
            }

            token_client.transfer(&reserve_address, &borrower, &amount);
        }

        credit_line.utilized_amount = updated_utilized;
        env.storage().persistent().set(&borrower, &credit_line);

        publish_drawn_event(
            &env,
            DrawnEvent {
                borrower,
                amount,
                new_utilized_amount: updated_utilized,
                timestamp: env.ledger().timestamp(),
            },
        );

        clear_reentrancy_guard(&env);
        ()
    }

    /// Repays credit. Allowed when status is Active, Suspended, or Defaulted.
    pub fn repay_credit(env: Env, borrower: Address, amount: i128) {
        set_reentrancy_guard(&env);
        borrower.require_auth();

        if amount <= 0 {
            clear_reentrancy_guard(&env);
            panic!("amount must be positive");
        }

        let token_address: Option<Address> = env.storage().instance().get(&DataKey::LiquidityToken);
        let reserve_address: Address = env
            .storage()
            .instance()
            .get(&DataKey::LiquiditySource)
            .unwrap_or(env.current_contract_address());

        let mut credit_line: CreditLineData = env
            .storage()
            .persistent()
            .get(&borrower)
            .expect("Credit line not found");

        if credit_line.status == CreditStatus::Closed {
            clear_reentrancy_guard(&env);
            panic!("credit line is closed");
        }

        let utilized = credit_line.utilized_amount.max(0);
        let applied_amount = if amount > utilized { utilized } else { amount };

        if let Some(token_address) = token_address {
            if applied_amount > 0 {
                let token_client = token::Client::new(&env, &token_address);
                token_client.transfer_from(
                    &env.current_contract_address(),
                    &borrower,
                    &reserve_address,
                    &applied_amount,
                );
            }
        }

        let new_utilized = checked_sub_i128(&env, utilized, applied_amount);
        credit_line.utilized_amount = new_utilized;
        env.storage().persistent().set(&borrower, &credit_line);

        publish_repayment_event(
            &env,
            RepaymentEvent {
                borrower,
                amount: applied_amount,
                new_utilized_amount: new_utilized,
                timestamp: env.ledger().timestamp(),
            },
        );

        clear_reentrancy_guard(&env);
    }

    /// Updates risk parameters for an existing credit line (admin only).
    pub fn update_risk_parameters(
        env: Env,
        borrower: Address,
        credit_limit: i128,
        interest_rate_bps: u32,
        risk_score: u32,
    ) {
        require_admin_auth(&env);

        let mut credit_line: CreditLineData = env
            .storage()
            .persistent()
            .get(&borrower)
            .expect("Credit line not found");

        if credit_limit < 0 {
            panic!("credit_limit must be non-negative");
        }
        if credit_limit < credit_line.utilized_amount {
            panic!("credit_limit cannot be less than utilized amount");
        }
        if interest_rate_bps > MAX_INTEREST_RATE_BPS {
            panic!("interest_rate_bps exceeds maximum");
        }
        if risk_score > MAX_RISK_SCORE {
            panic!("risk_score exceeds maximum");
        }

        if let Some(cfg) = env
            .storage()
            .instance()
            .get::<Symbol, RateChangeConfig>(&rate_cfg_key(&env))
        {
            if interest_rate_bps != credit_line.interest_rate_bps {
                let rate_delta = interest_rate_bps.abs_diff(credit_line.interest_rate_bps);
                if rate_delta > cfg.max_rate_change_bps {
                    panic!("rate change exceeds maximum allowed delta");
                }

                if credit_line.last_rate_update_ts > 0 && cfg.rate_change_min_interval > 0 {
                    let now = env.ledger().timestamp();
                    let elapsed = now.saturating_sub(credit_line.last_rate_update_ts);
                    if elapsed < cfg.rate_change_min_interval {
                        panic!("rate change too soon: minimum interval not elapsed");
                    }
                }

                credit_line.last_rate_update_ts = env.ledger().timestamp();
            }
        }

        credit_line.credit_limit = credit_limit;
        credit_line.interest_rate_bps = interest_rate_bps;
        credit_line.risk_score = risk_score;
        env.storage().persistent().set(&borrower, &credit_line);

        publish_risk_parameters_updated(
            &env,
            RiskParametersUpdatedEvent {
                borrower,
                credit_limit,
                interest_rate_bps,
                risk_score,
            },
        );
    }

    /// Sets rate-change limits (admin only).
    pub fn set_rate_change_limits(
        env: Env,
        max_rate_change_bps: u32,
        rate_change_min_interval: u64,
    ) {
        require_admin_auth(&env);
        let cfg = RateChangeConfig {
            max_rate_change_bps,
            rate_change_min_interval,
        };
        env.storage().instance().set(&rate_cfg_key(&env), &cfg);
    }

    /// Returns current rate-change limit configuration.
    pub fn get_rate_change_limits(env: Env) -> Option<RateChangeConfig> {
        env.storage().instance().get(&rate_cfg_key(&env))
    }

    /// Suspends a credit line temporarily (admin only).
    pub fn suspend_credit_line(env: Env, borrower: Address) {
        require_admin_auth(&env);
        let mut credit_line: CreditLineData = env
            .storage()
            .persistent()
            .get(&borrower)
            .expect("Credit line not found");

        credit_line.status = CreditStatus::Suspended;
        env.storage().persistent().set(&borrower, &credit_line);

        publish_credit_line_event(
            &env,
            (symbol_short!("credit"), symbol_short!("suspend")),
            CreditLineEvent {
                event_type: symbol_short!("suspend"),
                borrower,
                status: CreditStatus::Suspended,
                credit_limit: credit_line.credit_limit,
                interest_rate_bps: credit_line.interest_rate_bps,
                risk_score: credit_line.risk_score,
            },
        );
    }

    /// Closes a credit line. Callable by admin, or borrower when utilization is zero.
    pub fn close_credit_line(env: Env, borrower: Address, closer: Address) {
        closer.require_auth();

        let admin = require_admin(&env);

        let mut credit_line: CreditLineData = env
            .storage()
            .persistent()
            .get(&borrower)
            .expect("Credit line not found");

        if credit_line.status == CreditStatus::Closed {
            return;
        }

        let allowed = closer == admin || (closer == borrower && credit_line.utilized_amount == 0);

        if !allowed {
            if closer == borrower {
                panic!("cannot close: utilized amount not zero");
            }
            panic!("unauthorized");
        }

        credit_line.status = CreditStatus::Closed;
        env.storage().persistent().set(&borrower, &credit_line);

        publish_credit_line_event(
            &env,
            (symbol_short!("credit"), symbol_short!("closed")),
            CreditLineEvent {
                event_type: symbol_short!("closed"),
                borrower,
                status: CreditStatus::Closed,
                credit_limit: credit_line.credit_limit,
                interest_rate_bps: credit_line.interest_rate_bps,
                risk_score: credit_line.risk_score,
            },
        );
    }

    /// Marks a credit line as defaulted (admin only).
    pub fn default_credit_line(env: Env, borrower: Address) {
        require_admin_auth(&env);
        let mut credit_line: CreditLineData = env
            .storage()
            .persistent()
            .get(&borrower)
            .expect("Credit line not found");

        credit_line.status = CreditStatus::Defaulted;
        env.storage().persistent().set(&borrower, &credit_line);

        publish_credit_line_event(
            &env,
            (symbol_short!("credit"), symbol_short!("default")),
            CreditLineEvent {
                event_type: symbol_short!("default"),
                borrower,
                status: CreditStatus::Defaulted,
                credit_limit: credit_line.credit_limit,
                interest_rate_bps: credit_line.interest_rate_bps,
                risk_score: credit_line.risk_score,
            },
        );
    }

    /// Reinstates a defaulted credit line to Active (admin only).
    pub fn reinstate_credit_line(env: Env, borrower: Address) {
        require_admin_auth(&env);

        let mut credit_line: CreditLineData = env
            .storage()
            .persistent()
            .get(&borrower)
            .expect("Credit line not found");

        if credit_line.status != CreditStatus::Defaulted {
            panic!("credit line is not defaulted");
        }

        credit_line.status = CreditStatus::Active;
        env.storage().persistent().set(&borrower, &credit_line);

        publish_credit_line_event(
            &env,
            (symbol_short!("credit"), symbol_short!("reinstate")),
            CreditLineEvent {
                event_type: symbol_short!("reinstate"),
                borrower,
                status: CreditStatus::Active,
                credit_limit: credit_line.credit_limit,
                interest_rate_bps: credit_line.interest_rate_bps,
                risk_score: credit_line.risk_score,
            },
        );
    }

    /// Gets credit line data for a borrower (view function).
    pub fn get_credit_line(env: Env, borrower: Address) -> Option<CreditLineData> {
        env.storage().persistent().get(&borrower)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::token::{self, StellarAssetClient};
    use soroban_sdk::{Address, Env, Symbol};

    fn setup_with_limit(
        env: &Env,
        credit_limit: i128,
    ) -> (Address, Address, Address, CreditClient<'_>) {
        env.mock_all_auths();

        let admin = Address::generate(env);
        let borrower = Address::generate(env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(env, &contract_id);

        client.init(&admin);
        client.open_credit_line(&borrower, &credit_limit, &300_u32, &70_u32);

        (admin, borrower, contract_id, client)
    }

    fn setup_token<'a>(env: &'a Env) -> (Address, token::Client<'a>, StellarAssetClient<'a>) {
        let token_admin = Address::generate(env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin);
        let token_address = token_id.address();
        (
            token_address.clone(),
            token::Client::new(env, &token_address),
            StellarAssetClient::new(env, &token_address),
        )
    }

    fn credit_line(env: &Env, contract_id: &Address, borrower: &Address) -> CreditLineData {
        CreditClient::new(env, contract_id)
            .get_credit_line(borrower)
            .expect("credit line must exist")
    }

    fn set_reentrancy_flag(env: &Env, contract_id: &Address, value: bool) {
        env.as_contract(contract_id, || {
            env.storage()
                .instance()
                .set(&Symbol::new(env, "reentrancy"), &value);
        });
    }

    fn force_last_rate_update_ts(env: &Env, contract_id: &Address, borrower: &Address, ts: u64) {
        env.as_contract(contract_id, || {
            let mut line: CreditLineData = env
                .storage()
                .persistent()
                .get(borrower)
                .expect("line must exist");
            line.last_rate_update_ts = ts;
            env.storage().persistent().set(borrower, &line);
        });
    }

    #[test]
    fn test_init_sets_default_liquidity_source() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);

        let source: Address = env
            .as_contract(&contract_id, || {
                env.storage().instance().get(&DataKey::LiquiditySource)
            })
            .expect("liquidity source must be set");
        assert_eq!(source, contract_id);
    }

    #[test]
    #[should_panic(expected = "admin not set")]
    fn test_admin_methods_fail_before_init() {
        let env = Env::default();
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        let reserve = Address::generate(&env);
        client.set_liquidity_source(&reserve);
    }

    #[test]
    fn test_set_liquidity_token_and_source_admin_success() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let reserve = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        let (token_addr, _, _) = setup_token(&env);
        client.init(&admin);
        client.set_liquidity_token(&token_addr);
        client.set_liquidity_source(&reserve);

        let stored_token: Address = env
            .as_contract(&contract_id, || {
                env.storage().instance().get(&DataKey::LiquidityToken)
            })
            .expect("token must be set");
        let stored_source: Address = env
            .as_contract(&contract_id, || {
                env.storage().instance().get(&DataKey::LiquiditySource)
            })
            .expect("source must be set");
        assert_eq!(stored_token, token_addr);
        assert_eq!(stored_source, reserve);
    }

    #[test]
    #[should_panic]
    fn test_set_liquidity_token_requires_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        let (token_addr, _, _) = setup_token(&env);
        client.init(&admin);
        client.set_liquidity_token(&token_addr);
    }

    #[test]
    #[should_panic]
    fn test_set_liquidity_source_requires_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let reserve = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.set_liquidity_source(&reserve);
    }

    #[test]
    fn test_open_credit_line_allows_reopen_after_suspend() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&borrower, &1000_i128, &300_u32, &70_u32);
        client.suspend_credit_line(&borrower);
        client.open_credit_line(&borrower, &2000_i128, &450_u32, &55_u32);

        let line = credit_line(&env, &contract_id, &borrower);
        assert_eq!(line.credit_limit, 2000);
        assert_eq!(line.interest_rate_bps, 450);
        assert_eq!(line.risk_score, 55);
        assert_eq!(line.status, CreditStatus::Active);
    }

    #[test]
    #[should_panic(expected = "borrower already has an active credit line")]
    fn test_open_credit_line_duplicate_active_reverts() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.open_credit_line(&borrower, &2000_i128, &300_u32, &70_u32);
    }

    #[test]
    #[should_panic(expected = "credit_limit must be greater than zero")]
    fn test_open_credit_line_zero_limit_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&borrower, &0_i128, &300_u32, &70_u32);
    }

    #[test]
    #[should_panic(expected = "credit_limit must be greater than zero")]
    fn test_open_credit_line_negative_limit_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&borrower, &-1_i128, &300_u32, &70_u32);
    }

    #[test]
    #[should_panic(expected = "interest_rate_bps cannot exceed 10000 (100%)")]
    fn test_open_credit_line_interest_rate_too_high_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&borrower, &1000_i128, &10_001_u32, &70_u32);
    }

    #[test]
    #[should_panic(expected = "risk_score must be between 0 and 100")]
    fn test_open_credit_line_risk_score_too_high_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&borrower, &1000_i128, &300_u32, &101_u32);
    }

    #[test]
    fn test_draw_credit_near_i128_max_succeeds_without_overflow() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, i128::MAX);

        client.draw_credit(&borrower, &(i128::MAX - 1));
        client.draw_credit(&borrower, &1_i128);

        let credit_line = client
            .get_credit_line(&borrower)
            .expect("credit line must exist");
        assert_eq!(credit_line.utilized_amount, i128::MAX);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #12)")]
    fn test_draw_credit_overflow_reverts_with_defined_error() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, i128::MAX);

        client.draw_credit(&borrower, &(i128::MAX - 5));
        client.draw_credit(&borrower, &10_i128);
    }

    #[test]
    #[should_panic(expected = "exceeds credit limit")]
    fn test_draw_credit_large_values_exceed_limit_reverts_with_defined_error() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, i128::MAX - 1);

        client.draw_credit(&borrower, &(i128::MAX - 2));
        client.draw_credit(&borrower, &2_i128);
    }

    #[test]
    fn test_repay_credit_large_amount_caps_at_zero_without_underflow() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, i128::MAX);

        client.draw_credit(&borrower, &(i128::MAX - 100));
        client.repay_credit(&borrower, &i128::MAX);

        let credit_line = client
            .get_credit_line(&borrower)
            .expect("credit line must exist");
        assert_eq!(credit_line.utilized_amount, 0);
    }

    #[test]
    #[should_panic(expected = "credit_limit cannot be less than utilized amount")]
    fn test_update_risk_parameters_rejects_limit_below_utilized_near_i128_max() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, i128::MAX);

        client.draw_credit(&borrower, &(i128::MAX - 10));
        client.update_risk_parameters(&borrower, &(i128::MAX - 11), &300_u32, &70_u32);
    }

    #[test]
    fn test_draw_credit_updates_utilized_without_token() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.draw_credit(&borrower, &300_i128);
        assert_eq!(
            credit_line(&env, &contract_id, &borrower).utilized_amount,
            300
        );
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_draw_credit_rejects_zero_amount() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.draw_credit(&borrower, &0_i128);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_draw_credit_rejects_negative_amount() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.draw_credit(&borrower, &-1_i128);
    }

    #[test]
    #[should_panic(expected = "Credit line not found")]
    fn test_draw_credit_rejects_missing_line() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.draw_credit(&borrower, &10_i128);
    }

    #[test]
    #[should_panic(expected = "credit line is closed")]
    fn test_draw_credit_rejects_closed_line() {
        let env = Env::default();
        let (admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.close_credit_line(&borrower, &admin);
        client.draw_credit(&borrower, &1_i128);
    }

    #[test]
    #[should_panic(expected = "Credit line not active")]
    fn test_draw_credit_rejects_suspended_line() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.suspend_credit_line(&borrower);
        client.draw_credit(&borrower, &1_i128);
    }

    #[test]
    #[should_panic(expected = "Credit line not active")]
    fn test_draw_credit_rejects_defaulted_line() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.default_credit_line(&borrower);
        client.draw_credit(&borrower, &1_i128);
    }

    #[test]
    #[should_panic(expected = "exceeds credit limit")]
    fn test_draw_credit_rejects_over_limit() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.draw_credit(&borrower, &1001_i128);
    }

    #[test]
    fn test_draw_credit_with_contract_liquidity_source_transfers_tokens() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        let (token_addr, token_client, sac) = setup_token(&env);
        client.set_liquidity_token(&token_addr);
        sac.mint(&contract_id, &500_i128);
        client.draw_credit(&borrower, &200_i128);
        assert_eq!(token_client.balance(&contract_id), 300_i128);
        assert_eq!(token_client.balance(&borrower), 200_i128);
    }

    #[test]
    fn test_draw_credit_with_configured_liquidity_source_transfers_tokens() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        let reserve = contract_id.clone();
        let (token_addr, token_client, sac) = setup_token(&env);
        client.set_liquidity_token(&token_addr);
        client.set_liquidity_source(&reserve);
        sac.mint(&reserve, &700_i128);
        client.draw_credit(&borrower, &250_i128);
        assert_eq!(token_client.balance(&reserve), 450_i128);
        assert_eq!(token_client.balance(&borrower), 250_i128);
    }

    #[test]
    #[should_panic(expected = "Insufficient liquidity reserve for requested draw amount")]
    fn test_draw_credit_with_insufficient_liquidity_reverts() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        let (token_addr, _token_client, sac) = setup_token(&env);
        client.set_liquidity_token(&token_addr);
        sac.mint(&contract_id, &20_i128);
        client.draw_credit(&borrower, &30_i128);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #11)")]
    fn test_draw_credit_reentrancy_guard_reverts() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        set_reentrancy_flag(&env, &contract_id, true);
        client.draw_credit(&borrower, &1_i128);
    }

    #[test]
    fn test_repay_credit_reduces_utilized_without_token() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.draw_credit(&borrower, &400_i128);
        client.repay_credit(&borrower, &150_i128);
        assert_eq!(
            credit_line(&env, &contract_id, &borrower).utilized_amount,
            250
        );
    }

    #[test]
    fn test_repay_credit_caps_to_zero_when_amount_exceeds_utilized() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.draw_credit(&borrower, &100_i128);
        client.repay_credit(&borrower, &500_i128);
        assert_eq!(
            credit_line(&env, &contract_id, &borrower).utilized_amount,
            0
        );
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_repay_credit_rejects_zero_amount() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.repay_credit(&borrower, &0_i128);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_repay_credit_rejects_negative_amount() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.repay_credit(&borrower, &-1_i128);
    }

    #[test]
    #[should_panic(expected = "Credit line not found")]
    fn test_repay_credit_rejects_missing_line() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.repay_credit(&borrower, &10_i128);
    }

    #[test]
    #[should_panic(expected = "credit line is closed")]
    fn test_repay_credit_rejects_closed_line() {
        let env = Env::default();
        let (admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.close_credit_line(&borrower, &admin);
        client.repay_credit(&borrower, &10_i128);
    }

    #[test]
    fn test_repay_credit_allowed_when_defaulted() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.draw_credit(&borrower, &300_i128);
        client.default_credit_line(&borrower);
        client.repay_credit(&borrower, &100_i128);
        let line = credit_line(&env, &contract_id, &borrower);
        assert_eq!(line.status, CreditStatus::Defaulted);
        assert_eq!(line.utilized_amount, 200);
    }

    #[test]
    fn test_repay_credit_with_token_transfer_from_moves_funds() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        let (token_addr, token_client, sac) = setup_token(&env);
        client.set_liquidity_token(&token_addr);
        sac.mint(&contract_id, &1000_i128);
        client.draw_credit(&borrower, &300_i128);
        token_client.approve(&borrower, &contract_id, &200_i128, &1000_u32);
        client.repay_credit(&borrower, &200_i128);
        assert_eq!(token_client.balance(&borrower), 100_i128);
        assert_eq!(token_client.balance(&contract_id), 900_i128);
        assert_eq!(
            credit_line(&env, &contract_id, &borrower).utilized_amount,
            100_i128
        );
    }

    #[test]
    fn test_repay_credit_zero_utilized_with_token_skips_transfer() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        let (token_addr, token_client, _sac) = setup_token(&env);
        client.set_liquidity_token(&token_addr);
        client.repay_credit(&borrower, &10_i128);
        assert_eq!(token_client.balance(&borrower), 0_i128);
        assert_eq!(token_client.balance(&contract_id), 0_i128);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #11)")]
    fn test_repay_credit_reentrancy_guard_reverts() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        set_reentrancy_flag(&env, &contract_id, true);
        client.repay_credit(&borrower, &1_i128);
    }

    #[test]
    fn test_update_risk_parameters_success() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.update_risk_parameters(&borrower, &1500_i128, &400_u32, &80_u32);
        let line = credit_line(&env, &contract_id, &borrower);
        assert_eq!(line.credit_limit, 1500);
        assert_eq!(line.interest_rate_bps, 400);
        assert_eq!(line.risk_score, 80);
    }

    #[test]
    #[should_panic]
    fn test_update_risk_parameters_requires_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&borrower, &1000_i128, &300_u32, &70_u32);
        client.update_risk_parameters(&borrower, &1200_i128, &350_u32, &75_u32);
    }

    #[test]
    #[should_panic(expected = "Credit line not found")]
    fn test_update_risk_parameters_missing_line_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.update_risk_parameters(&borrower, &1000_i128, &300_u32, &70_u32);
    }

    #[test]
    #[should_panic(expected = "credit_limit must be non-negative")]
    fn test_update_risk_parameters_negative_limit_reverts() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.update_risk_parameters(&borrower, &-1_i128, &300_u32, &70_u32);
    }

    #[test]
    #[should_panic(expected = "interest_rate_bps exceeds maximum")]
    fn test_update_risk_parameters_interest_rate_too_high_reverts() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.update_risk_parameters(&borrower, &1000_i128, &10_001_u32, &70_u32);
    }

    #[test]
    #[should_panic(expected = "risk_score exceeds maximum")]
    fn test_update_risk_parameters_risk_score_too_high_reverts() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.update_risk_parameters(&borrower, &1000_i128, &300_u32, &101_u32);
    }

    #[test]
    fn test_set_and_get_rate_change_limits() {
        let env = Env::default();
        let (_admin, _borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.set_rate_change_limits(&250_u32, &3600_u64);
        let cfg = client
            .get_rate_change_limits()
            .expect("rate change config should exist");
        assert_eq!(cfg.max_rate_change_bps, 250_u32);
        assert_eq!(cfg.rate_change_min_interval, 3600_u64);
    }

    #[test]
    fn test_get_rate_change_limits_returns_none_when_unset() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        assert_eq!(client.get_rate_change_limits(), None);
    }

    #[test]
    #[should_panic(expected = "rate change exceeds maximum allowed delta")]
    fn test_update_risk_parameters_respects_rate_delta_limit() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.set_rate_change_limits(&10_u32, &0_u64);
        client.update_risk_parameters(&borrower, &1000_i128, &500_u32, &70_u32);
    }

    #[test]
    #[should_panic(expected = "rate change too soon: minimum interval not elapsed")]
    fn test_update_risk_parameters_respects_min_interval() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.set_rate_change_limits(&1000_u32, &100_u64);
        force_last_rate_update_ts(&env, &contract_id, &borrower, 10_u64);
        client.update_risk_parameters(&borrower, &1000_i128, &350_u32, &70_u32);
    }

    #[test]
    fn test_update_risk_parameters_unchanged_rate_skips_interval_check() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.set_rate_change_limits(&1_u32, &100_u64);
        force_last_rate_update_ts(&env, &contract_id, &borrower, 999_u64);
        client.update_risk_parameters(&borrower, &1200_i128, &300_u32, &75_u32);
        let line = credit_line(&env, &contract_id, &borrower);
        assert_eq!(line.credit_limit, 1200_i128);
        assert_eq!(line.last_rate_update_ts, 999_u64);
    }

    #[test]
    fn test_update_risk_parameters_rate_change_sets_last_update_timestamp() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        env.ledger().set_timestamp(12345_u64);
        client.set_rate_change_limits(&500_u32, &0_u64);
        client.update_risk_parameters(&borrower, &1000_i128, &400_u32, &70_u32);
        assert_eq!(
            credit_line(&env, &contract_id, &borrower).last_rate_update_ts,
            12345_u64
        );
    }

    #[test]
    fn test_suspend_credit_line_success() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.suspend_credit_line(&borrower);
        assert_eq!(
            credit_line(&env, &contract_id, &borrower).status,
            CreditStatus::Suspended
        );
    }

    #[test]
    #[should_panic]
    fn test_suspend_credit_line_requires_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&borrower, &1000_i128, &300_u32, &70_u32);
        client.suspend_credit_line(&borrower);
    }

    #[test]
    #[should_panic(expected = "Credit line not found")]
    fn test_suspend_credit_line_missing_line_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.suspend_credit_line(&borrower);
    }

    #[test]
    fn test_close_credit_line_admin_success() {
        let env = Env::default();
        let (admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.close_credit_line(&borrower, &admin);
        assert_eq!(
            credit_line(&env, &contract_id, &borrower).status,
            CreditStatus::Closed
        );
    }

    #[test]
    fn test_close_credit_line_borrower_success_when_zero_utilization() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.close_credit_line(&borrower, &borrower);
        assert_eq!(
            credit_line(&env, &contract_id, &borrower).status,
            CreditStatus::Closed
        );
    }

    #[test]
    #[should_panic(expected = "cannot close: utilized amount not zero")]
    fn test_close_credit_line_borrower_rejects_non_zero_utilization() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.draw_credit(&borrower, &1_i128);
        client.close_credit_line(&borrower, &borrower);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_close_credit_line_rejects_unauthorized_other() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        let other = Address::generate(&env);
        client.close_credit_line(&borrower, &other);
    }

    #[test]
    fn test_close_credit_line_idempotent_when_already_closed() {
        let env = Env::default();
        let (admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.close_credit_line(&borrower, &admin);
        client.close_credit_line(&borrower, &admin);
        assert_eq!(
            credit_line(&env, &contract_id, &borrower).status,
            CreditStatus::Closed
        );
    }

    #[test]
    fn test_default_credit_line_success() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.default_credit_line(&borrower);
        assert_eq!(
            credit_line(&env, &contract_id, &borrower).status,
            CreditStatus::Defaulted
        );
    }

    #[test]
    #[should_panic]
    fn test_default_credit_line_requires_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&borrower, &1000_i128, &300_u32, &70_u32);
        client.default_credit_line(&borrower);
    }

    #[test]
    #[should_panic(expected = "Credit line not found")]
    fn test_default_credit_line_missing_line_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.default_credit_line(&borrower);
    }

    #[test]
    fn test_reinstate_credit_line_success() {
        let env = Env::default();
        let (_admin, borrower, contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.default_credit_line(&borrower);
        client.reinstate_credit_line(&borrower);
        assert_eq!(
            credit_line(&env, &contract_id, &borrower).status,
            CreditStatus::Active
        );
    }

    #[test]
    #[should_panic(expected = "credit line is not defaulted")]
    fn test_reinstate_credit_line_rejects_non_defaulted() {
        let env = Env::default();
        let (_admin, borrower, _contract_id, client) = setup_with_limit(&env, 1000_i128);
        client.reinstate_credit_line(&borrower);
    }

    #[test]
    #[should_panic]
    fn test_reinstate_credit_line_requires_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&borrower, &1000_i128, &300_u32, &70_u32);
        client.default_credit_line(&borrower);
        client.reinstate_credit_line(&borrower);
    }

    #[test]
    #[should_panic(expected = "Credit line not found")]
    fn test_reinstate_credit_line_missing_line_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.reinstate_credit_line(&borrower);
    }

    #[test]
    fn test_get_credit_line_returns_none_for_missing_borrower() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        assert!(client.get_credit_line(&borrower).is_none());
    }

    #[test]
    fn test_all_status_variants_constructible() {
        let variants = [
            CreditStatus::Active,
            CreditStatus::Suspended,
            CreditStatus::Defaulted,
            CreditStatus::Closed,
        ];
        assert_eq!(variants.len(), 4);
    }

    #[test]
    fn test_contract_error_overflow_discriminant() {
        assert_eq!(ContractError::Overflow as u32, 12);
    }
}
