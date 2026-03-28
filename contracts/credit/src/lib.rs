#![no_std]
#![allow(clippy::unused_unit)]

//! Creditra credit contract: credit lines, draw/repay, risk parameters.
//!
//! # Status transitions
//!
//! | From    | To        | Trigger |
//! |---------|-----------|---------|
//! | Active  | Defaulted | Admin calls `default_credit_line` (e.g. after past-due or oracle signal). |
//! | Suspended | Defaulted | Admin calls `default_credit_line`. |
//! | Defaulted | Active   | Admin calls `reinstate_credit_line`. |
//! | Defaulted | Suspended | Admin calls `suspend_credit_line`. |
//! | Defaulted | Closed   | Admin or borrower (when utilized_amount == 0) calls `close_credit_line`. |
//!
//! When status is Defaulted: `draw_credit` is disabled; `repay_credit` is allowed.
//!
//! # Reentrancy
//! Soroban token transfers (e.g. Stellar Asset Contract) do not invoke callbacks back into
//! the caller. This contract uses a reentrancy guard on draw_credit and repay_credit as a
//! defense-in-depth measure; if a token or future integration ever called back, the guard
//! would revert.

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
use types::{CreditLineData, CreditStatus, RateChangeConfig};

/// Maximum interest rate in basis points (100%).
const MAX_INTEREST_RATE_BPS: u32 = 10_000;

/// Maximum risk score (0–100 scale).
const MAX_RISK_SCORE: u32 = 100;

/// Instance storage key for reentrancy guard.
fn reentrancy_key(env: &Env) -> Symbol {
    Symbol::new(env, "reentrancy")
}

/// Instance storage key for admin.
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

/// Assert reentrancy guard is not set; set it for the duration of the call.
/// Caller must call clear_reentrancy_guard when done (on all paths).
fn set_reentrancy_guard(env: &Env) {
    let key = reentrancy_key(env);
    let current: bool = env.storage().instance().get(&key).unwrap_or(false);
    if current {
        panic!("reentrancy guard");
    }
    env.storage().instance().set(&key, &true);
}

fn clear_reentrancy_guard(env: &Env) {
    env.storage().instance().set(&reentrancy_key(env), &false);
}

#[contract]
pub struct Credit;

#[contractimpl]
impl Credit {
    /// @notice Initializes contract-level configuration.
    /// @dev Sets admin and defaults liquidity source to this contract address.
    pub fn init(env: Env, admin: Address) -> () {
        env.storage().instance().set(&admin_key(&env), &admin);
        env.storage()
            .instance()
            .set(&DataKey::LiquiditySource, &env.current_contract_address());
        ()
    }

    /// @notice Sets the token contract used for reserve/liquidity checks and draw transfers.
    /// @dev Admin-only.
    pub fn set_liquidity_token(env: Env, token_address: Address) -> () {
        require_admin_auth(&env);
        env.storage()
            .instance()
            .set(&DataKey::LiquidityToken, &token_address);
        ()
    }

    /// @notice Sets the address that provides liquidity for draw operations.
    /// @dev Admin-only. If unset, init config uses the contract address.
    pub fn set_liquidity_source(env: Env, reserve_address: Address) -> () {
        require_admin_auth(&env);
        env.storage()
            .instance()
            .set(&DataKey::LiquiditySource, &reserve_address);
        ()
    }

    /// Open a new credit line for a borrower (called by backend/risk engine).
    ///
    /// # Arguments
    /// * `borrower` - The address of the borrower
    /// * `credit_limit` - Maximum borrowable amount (must be > 0)
    /// * `interest_rate_bps` - Annual interest rate in basis points (max 10000 = 100%)
    /// * `risk_score` - Borrower risk score (0–100)
    ///
    /// # Panics
    /// * If `credit_limit` <= 0
    /// * If `interest_rate_bps` > 10000
    /// * If `risk_score` > 100
    /// * If an Active credit line already exists for the borrower
    ///
    /// # Events
    /// Emits `(credit, opened)` with a `CreditLineEvent` payload.
    pub fn open_credit_line(
        env: Env,
        borrower: Address,
        credit_limit: i128,
        interest_rate_bps: u32,
        risk_score: u32,
    ) {
        assert!(credit_limit > 0, "credit_limit must be greater than zero");
        assert!(
            interest_rate_bps <= 10_000,
            "interest_rate_bps cannot exceed 10000 (100%)"
        );
        assert!(risk_score <= 100, "risk_score must be between 0 and 100");

        // Prevent overwriting an existing Active credit line
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
                borrower: borrower.clone(),
                status: CreditStatus::Active,
                credit_limit,
                interest_rate_bps,
                risk_score,
            },
        );
    }

    /// @notice Draws credit by transferring liquidity tokens to the borrower.
    /// @dev Enforces status/limit/liquidity checks and uses a reentrancy guard.
    /// Reverts if status is not Active (e.g. Suspended, Defaulted, or Closed).
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

        if credit_line.status != CreditStatus::Active {
            clear_reentrancy_guard(&env);
            if credit_line.status == CreditStatus::Closed {
                panic!("credit line is closed");
            } else if credit_line.status == CreditStatus::Suspended {
                panic!("credit line is suspended");
            } else if credit_line.status == CreditStatus::Defaulted {
                panic!("credit line is defaulted");
            }
            panic!("credit line is not active");
        }

        let updated_utilized = credit_line
            .utilized_amount
            .checked_add(amount)
            .expect("overflow");

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
        let timestamp = env.ledger().timestamp();
        publish_drawn_event(
            &env,
            DrawnEvent {
                borrower,
                amount,
                new_utilized_amount: updated_utilized,
                timestamp,
            },
        );
        clear_reentrancy_guard(&env);
        ()
    }

    /// Repay credit (borrower).
    /// Allowed when status is Active, Suspended, or Defaulted. Reverts if credit line does not exist,
    /// is Closed, or borrower has not authorized. Reduces utilized_amount by amount (capped at 0).
    pub fn repay_credit(env: Env, borrower: Address, amount: i128) {
        set_reentrancy_guard(&env);
        borrower.require_auth();
        let mut credit_line: CreditLineData = env
            .storage()
            .persistent()
            .get(&borrower)
            .expect("Credit line not found");

        if credit_line.borrower != borrower {
            panic!("Borrower mismatch for credit line");
        }

        if credit_line.status == CreditStatus::Closed {
            clear_reentrancy_guard(&env);
            panic!("credit line is closed");
        }
        if amount <= 0 {
            clear_reentrancy_guard(&env);
            panic!("amount must be positive");
        }

        // Token transfer logic (if token is set)
        let token_address: Option<Address> = env.storage().instance().get(&DataKey::LiquidityToken);
        if let Some(token_address) = token_address {
            let reserve_address: Address = env
                .storage()
                .instance()
                .get(&DataKey::LiquiditySource)
                .unwrap_or(env.current_contract_address());
            
            let token_client = token::Client::new(&env, &token_address);
            let effective_amount = amount.min(credit_line.utilized_amount);
            if effective_amount > 0 {
                token_client.transfer(&borrower, &reserve_address, &effective_amount);
            }
        }

        let new_utilized = credit_line.utilized_amount.saturating_sub(amount).max(0);
        credit_line.utilized_amount = new_utilized;
        env.storage().persistent().set(&borrower, &credit_line);

        let timestamp = env.ledger().timestamp();
        publish_repayment_event(
            &env,
            RepaymentEvent {
                borrower: borrower.clone(),
                amount,
                new_utilized_amount: new_utilized,
                timestamp,
            },
        );
        clear_reentrancy_guard(&env);
    }

    /// Update risk parameters for an existing credit line (admin only).
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

        credit_line.credit_limit = credit_limit;
        credit_line.interest_rate_bps = interest_rate_bps;
        credit_line.risk_score = risk_score;
        env.storage().persistent().set(&borrower, &credit_line);

        publish_risk_parameters_updated(
            &env,
            RiskParametersUpdatedEvent {
                borrower: borrower.clone(),
                credit_limit,
                interest_rate_bps,
                risk_score,
            },
        );
    }

    /// Set rate-change limits (admin only).
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

    /// Get the current rate-change limit configuration (view function).
    pub fn get_rate_change_limits(env: Env) -> Option<RateChangeConfig> {
        env.storage().instance().get(&rate_cfg_key(&env))
    }

    /// Suspend a credit line temporarily (admin only).
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
                borrower: borrower.clone(),
                status: CreditStatus::Suspended,
                credit_limit: credit_line.credit_limit,
                interest_rate_bps: credit_line.interest_rate_bps,
                risk_score: credit_line.risk_score,
            },
        );
    }

    /// Close a credit line. Callable by admin (force-close) or by borrower when utilization is zero.
    pub fn close_credit_line(env: Env, borrower: Address, closer: Address) {
        closer.require_auth();

        let admin: Address = require_admin(&env);

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
                borrower: borrower.clone(),
                status: CreditStatus::Closed,
                credit_limit: credit_line.credit_limit,
                interest_rate_bps: credit_line.interest_rate_bps,
                risk_score: credit_line.risk_score,
            },
        );
    }

    /// Mark a credit line as defaulted (admin only).
    /// Call when the line is past due or when an oracle/off-chain signal indicates default.
    /// Transition: Active or Suspended → Defaulted.
    pub fn default_credit_line(env: Env, borrower: Address) {
        require_admin_auth(&env);
        let mut credit_line: CreditLineData = env
            .storage()
            .persistent()
            .get(&borrower)
            .expect("Credit line not found");

        if credit_line.status != CreditStatus::Active && credit_line.status != CreditStatus::Suspended {
            panic!("invalid source status for default");
        }

        credit_line.status = CreditStatus::Defaulted;
        env.storage().persistent().set(&borrower, &credit_line);

        publish_credit_line_event(
            &env,
            (symbol_short!("credit"), symbol_short!("default")),
            CreditLineEvent {
                event_type: symbol_short!("default"),
                borrower: borrower.clone(),
                status: CreditStatus::Defaulted,
                credit_limit: credit_line.credit_limit,
                interest_rate_bps: credit_line.interest_rate_bps,
                risk_score: credit_line.risk_score,
            },
        );
    }

    /// Reinstate a defaulted credit line to Active (admin only).
    /// Allowed only when status is Defaulted. Transition: Defaulted → Active.
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
                borrower: borrower.clone(),
                status: CreditStatus::Active,
                credit_limit: credit_line.credit_limit,
                interest_rate_bps: credit_line.interest_rate_bps,
                risk_score: credit_line.risk_score,
            },
        );
    }

    /// Get credit line data for a borrower (view function).
    pub fn get_credit_line(env: Env, borrower: Address) -> Option<CreditLineData> {
        env.storage().persistent().get(&borrower)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;

    fn setup_test(env: &Env) -> (Address, Address, CreditClient<'_>) {
        env.mock_all_auths();

        let admin = Address::generate(env);
        let borrower = Address::generate(env);

        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(env, &contract_id);

        client.init(&admin);
        client.open_credit_line(&borrower, &1000_i128, &300_u32, &70_u32);

        (admin, borrower, client)
    }

    fn setup_contract_with_credit_line<'a>(
        env: &'a Env,
        borrower: &'a Address,
        credit_limit: i128,
        reserve_amount: i128,
    ) -> (CreditClient<'a>, Address, Address) {
        let admin = Address::generate(env);
        let contract_id = env.register(Credit, ());
        let token_admin = Address::generate(env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin);
        let token_address = token_id.address();
        let client = CreditClient::new(env, &contract_id);
        client.init(&admin);
        if reserve_amount > 0 {
            let sac = StellarAssetClient::new(env, &token_address);
            sac.mint(&contract_id, &reserve_amount);
        }
        client.set_liquidity_token(&token_address);
        client.open_credit_line(borrower, &credit_limit, &300_u32, &70_u32);
        (client, token_address, admin)
    }

    #[test]
    fn test_init_and_open_credit_line() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);

        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);

        client.init(&admin);
        client.open_credit_line(&borrower, &1000_i128, &300_u32, &70_u32);

        let credit_line = client.get_credit_line(&borrower).unwrap();
        assert_eq!(credit_line.borrower, borrower);
        assert_eq!(credit_line.status, CreditStatus::Active);
    }

    #[test]
    fn test_draw_credit_within_limit() {
        let env = Env::default();
        let (_admin, borrower, client) = setup_test(&env);

        client.draw_credit(&borrower, &400_i128);

        let line = client.get_credit_line(&borrower).unwrap();
        assert_eq!(line.utilized_amount, 400);
    }

    #[test]
    #[should_panic(expected = "exceeds credit limit")]
    fn test_draw_credit_exceeds_limit() {
        let env = Env::default();
        let (_admin, borrower, client) = setup_test(&env);

        client.draw_credit(&borrower, &1001_i128);
    }

    #[test]
    fn test_repay_credit() {
        let env = Env::default();
        let (_admin, borrower, client) = setup_test(&env);

        client.draw_credit(&borrower, &500_i128);
        client.repay_credit(&borrower, &200_i128);

        let line = client.get_credit_line(&borrower).unwrap();
        assert_eq!(line.utilized_amount, 300);
    }

    #[test]
    fn test_suspend_credit_line() {
        let env = Env::default();
        let (_admin, borrower, client) = setup_test(&env);

        client.suspend_credit_line(&borrower);
        let line = client.get_credit_line(&borrower).unwrap();
        assert_eq!(line.status, CreditStatus::Suspended);
    }

    #[test]
    #[should_panic(expected = "credit line is suspended")]
    fn test_draw_credit_suspended_reverts() {
        let env = Env::default();
        let (_admin, borrower, client) = setup_test(&env);

        client.suspend_credit_line(&borrower);
        client.draw_credit(&borrower, &100_i128);
    }

    #[test]
    fn test_default_credit_line() {
        let env = Env::default();
        let (_admin, borrower, client) = setup_test(&env);

        client.default_credit_line(&borrower);
        let line = client.get_credit_line(&borrower).unwrap();
        assert_eq!(line.status, CreditStatus::Defaulted);
    }

    #[test]
    #[should_panic(expected = "credit line is defaulted")]
    fn test_draw_credit_defaulted_reverts() {
        let env = Env::default();
        let (_admin, borrower, client) = setup_test(&env);

        client.default_credit_line(&borrower);
        client.draw_credit(&borrower, &100_i128);
    }

    #[test]
    fn test_repay_credit_defaulted_allowed() {
        let env = Env::default();
        let (_admin, borrower, client) = setup_test(&env);

        client.draw_credit(&borrower, &500_i128);
        client.default_credit_line(&borrower);
        client.repay_credit(&borrower, &200_i128);

        let line = client.get_credit_line(&borrower).unwrap();
        assert_eq!(line.utilized_amount, 300);
        assert_eq!(line.status, CreditStatus::Defaulted);
    }

    #[test]
    #[should_panic(expected = "invalid source status for default")]
    fn test_default_credit_line_from_closed_reverts() {
        let env = Env::default();
        let (admin, borrower, client) = setup_test(&env);

        client.close_credit_line(&borrower, &admin);
        client.default_credit_line(&borrower);
    }

    #[test]
    fn test_reinstate_credit_line() {
        let env = Env::default();
        let (_admin, borrower, client) = setup_test(&env);

        client.default_credit_line(&borrower);
        client.reinstate_credit_line(&borrower);
        let line = client.get_credit_line(&borrower).unwrap();
        assert_eq!(line.status, CreditStatus::Active);
    }

    #[test]
    fn test_draw_credit_with_liquidity_token() {
        let env = Env::default();
        env.mock_all_auths();
        let borrower = Address::generate(&env);
        let (client, token_address, _admin) = setup_contract_with_credit_line(&env, &borrower, 1000, 1000);

        client.draw_credit(&borrower, &300_i128);
        
        let token_client = token::Client::new(&env, &token_address);
        assert_eq!(token_client.balance(&borrower), 300);
    }

    #[test]
    fn test_repay_credit_with_liquidity_token() {
        let env = Env::default();
        env.mock_all_auths();
        let borrower = Address::generate(&env);
        let (client, token_address, _admin) = setup_contract_with_credit_line(&env, &borrower, 1000, 1000);
        
        let sac = StellarAssetClient::new(&env, &token_address);
        sac.mint(&borrower, &500_i128);

        client.draw_credit(&borrower, &400_i128);
        client.repay_credit(&borrower, &200_i128);

        let line = client.get_credit_line(&borrower).unwrap();
        assert_eq!(line.utilized_amount, 200);
        
        let token_client = token::Client::new(&env, &token_address);
        // Initial 500 + drawn 400 - repaid 200 = 700
        assert_eq!(token_client.balance(&borrower), 700);
    }
}
