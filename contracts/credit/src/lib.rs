// SPDX-License-Identifier: MIT
#![cfg_attr(not(test), no_std)]
#![allow(clippy::unused_unit)]

//! Creditra credit contract: credit lines, draw/repay, risk parameters.

mod accrual;
#[cfg(test)]
mod accrual_tests;
mod auth;
mod config;
mod events;
mod freeze;
mod lifecycle;
mod query;
mod risk;
mod storage;
pub mod types;

#[cfg(test)]
mod boundary_tests;
#[cfg(test)]
mod risk_formula_tests;

use crate::auth::{require_admin, require_admin_auth};
use crate::events::{
    publish_admin_rotation_accepted, publish_admin_rotation_proposed, publish_drawn_event,
    publish_interest_accrued_event, publish_paused_event, publish_rate_formula_config_event,
    publish_repayment_event, AdminRotationAcceptedEvent, AdminRotationProposedEvent, DrawnEvent,
    InterestAccruedEvent, PausedEvent, RateFormulaConfigEvent, RepaymentEvent,
};
use crate::storage::{
    admin_key, assert_not_paused, clear_reentrancy_guard, proposed_admin_key, proposed_at_key,
    rate_cfg_key, rate_formula_key, set_reentrancy_guard, DataKey,
};
use crate::types::{
    ContractError, ContractVersion, CreditLineData, CreditStatus, GracePeriodConfig,
    GraceWaiverMode, RateChangeConfig, RateFormulaConfig,
};
use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol};

pub const CONTRACT_API_VERSION: (u32, u32, u32) = (1, 0, 0);

#[allow(dead_code)]
const SECONDS_PER_YEAR: u64 = 31_536_000;

const SCHEMA_VERSION: u32 = 1;

#[contract]
pub struct Credit;

#[contractimpl]
impl Credit {
    pub fn init(env: Env, admin: Address) {
        config::init(env, admin)
    }

    pub fn propose_admin(env: Env, new_admin: Address, delay_seconds: u64) {
        let current_admin = require_admin_auth(&env);
        let accept_after = env.ledger().timestamp().saturating_add(delay_seconds);

        env.storage()
            .instance()
            .set(&proposed_admin_key(&env), &new_admin);
        env.storage()
            .instance()
            .set(&proposed_at_key(&env), &accept_after);

        publish_admin_rotation_proposed(
            &env,
            AdminRotationProposedEvent {
                current_admin,
                proposed_admin: new_admin,
                accept_after,
            },
        );
    }

    pub fn accept_admin(env: Env) {
        let proposed_admin: Address = env
            .storage()
            .instance()
            .get(&proposed_admin_key(&env))
            .unwrap_or_else(|| panic!("no pending admin proposal"));
        let accept_after: u64 = env
            .storage()
            .instance()
            .get(&proposed_at_key(&env))
            .unwrap_or(0_u64);

        proposed_admin.require_auth();
        if env.ledger().timestamp() < accept_after {
            env.panic_with_error(ContractError::AdminAcceptTooEarly);
        }

        let previous_admin = require_admin(&env);
        env.storage()
            .instance()
            .set(&admin_key(&env), &proposed_admin);
        env.storage().instance().remove(&proposed_admin_key(&env));
        env.storage().instance().remove(&proposed_at_key(&env));

        publish_admin_rotation_accepted(
            &env,
            AdminRotationAcceptedEvent {
                previous_admin,
                new_admin: proposed_admin,
            },
        );
    }

    pub fn set_liquidity_token(env: Env, token_address: Address) {
        require_admin_auth(&env);
        env.storage()
            .instance()
            .set(&DataKey::LiquidityToken, &token_address);
    }

    pub fn set_liquidity_source(env: Env, reserve_address: Address) {
        require_admin_auth(&env);
        env.storage()
            .instance()
            .set(&DataKey::LiquiditySource, &reserve_address);
    }

    pub fn get_liquidity_source(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::LiquiditySource)
            .unwrap_or_else(|| env.current_contract_address())
    }

    pub fn open_credit_line(
        env: Env,
        borrower: Address,
        credit_limit: i128,
        interest_rate_bps: u32,
        risk_score: u32,
    ) {
        lifecycle::open_credit_line(env, borrower, credit_limit, interest_rate_bps, risk_score)
    }

    pub fn draw_credit(env: Env, borrower: Address, amount: i128) {
        assert_not_paused(&env);
        set_reentrancy_guard(&env);

        borrower.require_auth();

        if amount <= 0 {
            clear_reentrancy_guard(&env);
            panic!("amount must be positive");
        }

        if freeze::is_draws_frozen(&env) {
            clear_reentrancy_guard(&env);
            env.panic_with_error(ContractError::DrawsFrozen);
        }

        if let Some(max_draw) = env
            .storage()
            .instance()
            .get::<DataKey, i128>(&DataKey::MaxDrawAmount)
        {
            if amount > max_draw {
                clear_reentrancy_guard(&env);
                env.panic_with_error(ContractError::DrawExceedsMaxAmount);
            }
        }

        let mut credit_line: CreditLineData = env
            .storage()
            .persistent()
            .get(&borrower)
            .unwrap_or_else(|| {
                clear_reentrancy_guard(&env);
                env.panic_with_error(ContractError::CreditLineNotFound)
            });

        credit_line = accrual::apply_accrual(&env, credit_line);

        match credit_line.status {
            CreditStatus::Suspended => {
                clear_reentrancy_guard(&env);
                env.panic_with_error(ContractError::CreditLineSuspended);
            }
            CreditStatus::Defaulted => {
                clear_reentrancy_guard(&env);
                env.panic_with_error(ContractError::CreditLineDefaulted);
            }
            CreditStatus::Closed => {
                clear_reentrancy_guard(&env);
                env.panic_with_error(ContractError::CreditLineClosed);
            }
            CreditStatus::Active => {}
            _ => {
                clear_reentrancy_guard(&env);
                env.panic_with_error(ContractError::CreditLineClosed);
            }
        }

        let updated_utilized = credit_line
            .utilized_amount
            .checked_add(amount)
            .unwrap_or_else(|| {
                clear_reentrancy_guard(&env);
                env.panic_with_error(ContractError::Overflow)
            });

        if updated_utilized > credit_line.credit_limit {
            clear_reentrancy_guard(&env);
            env.panic_with_error(ContractError::OverLimit);
        }

        let maybe_token: Option<Address> =
            env.storage().instance().get(&DataKey::LiquidityToken);
        if let Some(token_address) = maybe_token {
            let reserve_address: Address = env
                .storage()
                .instance()
                .get(&DataKey::LiquiditySource)
                .unwrap_or_else(|| env.current_contract_address());

            let token_client = token::Client::new(&env, &token_address);
            let reserve_balance = token_client.balance(&reserve_address);
            if reserve_balance < amount {
                clear_reentrancy_guard(&env);
                env.panic_with_error(ContractError::InsufficientLiquidityReserve);
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
    }

    pub fn repay_credit(env: Env, borrower: Address, amount: i128) {
        set_reentrancy_guard(&env);
        borrower.require_auth();

        if amount <= 0 {
            clear_reentrancy_guard(&env);
            env.panic_with_error(ContractError::InvalidAmount);
        }

        let mut credit_line: CreditLineData = env
            .storage()
            .persistent()
            .get(&borrower)
            .unwrap_or_else(|| {
                clear_reentrancy_guard(&env);
                env.panic_with_error(ContractError::CreditLineNotFound)
            });

        credit_line = accrual::apply_accrual(&env, credit_line);

        if credit_line.status == CreditStatus::Closed {
            clear_reentrancy_guard(&env);
            env.panic_with_error(ContractError::CreditLineClosed);
        }

        let effective_repay = if amount > credit_line.utilized_amount {
            credit_line.utilized_amount
        } else {
            amount
        };

        if effective_repay > 0 {
            let maybe_token: Option<Address> =
                env.storage().instance().get(&DataKey::LiquidityToken);
            if let Some(token_address) = maybe_token {
                let reserve_address: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::LiquiditySource)
                    .unwrap_or_else(|| env.current_contract_address());

                let token_client = token::Client::new(&env, &token_address);
                let contract_address = env.current_contract_address();

                token_client.transfer_from(
                    &contract_address,
                    &borrower,
                    &reserve_address,
                    &effective_repay,
                );
            }
        }

        let interest_repaid = effective_repay.min(credit_line.accrued_interest);
        let principal_repaid = effective_repay - interest_repaid;
        credit_line.accrued_interest = credit_line
            .accrued_interest
            .checked_sub(interest_repaid)
            .unwrap_or(0);

        let new_utilized = credit_line
            .utilized_amount
            .saturating_sub(effective_repay)
            .max(0);
        credit_line.utilized_amount = new_utilized;

        env.storage().persistent().set(&borrower, &credit_line);

        let timestamp = env.ledger().timestamp();
        publish_interest_accrued_event(
            &env,
            InterestAccruedEvent {
                borrower: borrower.clone(),
                accrued_amount: 0,
                total_accrued_interest: credit_line.accrued_interest,
                new_utilized_amount: new_utilized,
                timestamp,
            },
        );
        publish_repayment_event(
            &env,
            RepaymentEvent {
                borrower: borrower.clone(),
                amount: effective_repay,
                interest_repaid,
                principal_repaid,
                new_utilized_amount: new_utilized,
                new_accrued_interest: credit_line.accrued_interest,
                timestamp,
            },
        );

        clear_reentrancy_guard(&env);
    }

    pub fn update_risk_parameters(
        env: Env,
        borrower: Address,
        credit_limit: i128,
        interest_rate_bps: u32,
        risk_score: u32,
    ) {
        risk::update_risk_parameters(env, borrower, credit_limit, interest_rate_bps, risk_score)
    }

    pub fn set_rate_change_limits(
        env: Env,
        max_rate_change_bps: u32,
        rate_change_min_interval: u64,
    ) {
        risk::set_rate_change_limits(env, max_rate_change_bps, rate_change_min_interval)
    }

    pub fn get_rate_change_limits(env: Env) -> Option<RateChangeConfig> {
        env.storage().instance().get(&rate_cfg_key(&env))
    }

    pub fn set_grace_period_config(
        env: Env,
        grace_period_seconds: u64,
        waiver_mode: GraceWaiverMode,
        reduced_rate_bps: u32,
    ) {
        require_admin_auth(&env);
        if reduced_rate_bps > crate::risk::MAX_INTEREST_RATE_BPS {
            env.panic_with_error(ContractError::RateTooHigh);
        }
        let cfg = GracePeriodConfig {
            grace_period_seconds,
            waiver_mode,
            reduced_rate_bps,
        };
        env.storage()
            .instance()
            .set(&crate::storage::grace_period_key(&env), &cfg);
    }

    pub fn get_grace_period_config(env: Env) -> Option<GracePeriodConfig> {
        env.storage()
            .instance()
            .get(&crate::storage::grace_period_key(&env))
    }

    pub fn set_max_draw_amount(env: Env, amount: i128) {
        assert_not_paused(&env);
        require_admin_auth(&env);
        if amount <= 0 {
            env.panic_with_error(ContractError::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxDrawAmount, &amount);
    }

    pub fn get_max_draw_amount(env: Env) -> Option<i128> {
        env.storage().instance().get(&DataKey::MaxDrawAmount)
    }

    pub fn get_schema_version(env: Env) -> Option<u32> {
        env.storage().instance().get(&DataKey::SchemaVersion)
    }

    pub fn suspend_credit_line(env: Env, borrower: Address) {
        lifecycle::suspend_credit_line(env, borrower)
    }

    pub fn self_suspend_credit_line(env: Env, borrower: Address) {
        lifecycle::self_suspend_credit_line(env, borrower)
    }

    pub fn close_credit_line(env: Env, borrower: Address, closer: Address) {
        lifecycle::close_credit_line(env, borrower, closer)
    }

    pub fn default_credit_line(env: Env, borrower: Address) {
        lifecycle::default_credit_line(env, borrower)
    }

    pub fn reinstate_credit_line(env: Env, borrower: Address, target_status: CreditStatus) {
        lifecycle::reinstate_credit_line(env, borrower, target_status)
    }

    pub fn settle_default_liquidation(
        env: Env,
        borrower: Address,
        recovered_amount: i128,
        settlement_id: Symbol,
    ) {
        lifecycle::settle_default_liquidation(env, borrower, recovered_amount, settlement_id)
    }

    pub fn get_credit_line(env: Env, borrower: Address) -> Option<CreditLineData> {
        query::get_credit_line(env, borrower)
    }

    pub fn freeze_draws(env: Env) {
        freeze::freeze_draws(env)
    }

    pub fn unfreeze_draws(env: Env) {
        freeze::unfreeze_draws(env)
    }

    pub fn is_draws_frozen(env: Env) -> bool {
        freeze::is_draws_frozen(&env)
    }

    pub fn set_protocol_paused(env: Env, paused: bool) {
        let admin = require_admin_auth(&env);
        storage::set_paused(&env, paused);
        publish_paused_event(
            &env,
            PausedEvent {
                paused,
                timestamp: env.ledger().timestamp(),
                actor: admin,
            },
        );
    }

    pub fn is_protocol_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    pub fn set_rate_formula_config(
        env: Env,
        base_rate_bps: u32,
        slope_bps_per_score: u32,
        min_rate_bps: u32,
        max_rate_bps: u32,
    ) {
        require_admin_auth(&env);
        assert!(
            min_rate_bps <= max_rate_bps,
            "min_rate_bps must be <= max_rate_bps"
        );
        assert!(
            max_rate_bps <= crate::risk::MAX_INTEREST_RATE_BPS,
            "max_rate_bps exceeds MAX_INTEREST_RATE_BPS"
        );
        assert!(
            base_rate_bps <= crate::risk::MAX_INTEREST_RATE_BPS,
            "base_rate_bps exceeds MAX_INTEREST_RATE_BPS"
        );
        let cfg = RateFormulaConfig {
            base_rate_bps,
            slope_bps_per_score,
            min_rate_bps,
            max_rate_bps,
        };
        env.storage()
            .instance()
            .set(&rate_formula_key(&env), &cfg);
        publish_rate_formula_config_event(&env, RateFormulaConfigEvent { enabled: true });
    }

    pub fn get_rate_formula_config(env: Env) -> Option<RateFormulaConfig> {
        risk::get_rate_formula_config(env)
    }

    pub fn clear_rate_formula_config(env: Env) {
        require_admin_auth(&env);
        env.storage().instance().remove(&rate_formula_key(&env));
        publish_rate_formula_config_event(&env, RateFormulaConfigEvent { enabled: false });
    }

    pub fn get_contract_version(_env: Env) -> ContractVersion {
        ContractVersion {
            major: CONTRACT_API_VERSION.0,
            minor: CONTRACT_API_VERSION.1,
            patch: CONTRACT_API_VERSION.2,
        }
    }
}
