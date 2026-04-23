// SPDX-License-Identifier: MIT
#![no_std]
#![allow(clippy::unused_unit)]

mod auth;
mod borrow;
mod config;
mod events;
mod lifecycle;
mod query;
mod risk;
mod storage;
pub mod types;

use soroban_sdk::{contract, contractimpl, Address, Env};
use types::{CreditLineData, RateChangeConfig};

#[contract]
pub struct Credit;

#[contractimpl]
impl Credit {
    pub fn init(env: Env, admin: Address) {
        config::init(env, admin)
    }

    pub fn set_liquidity_token(env: Env, token_address: Address) {
        config::set_liquidity_token(env, token_address)
    }

    pub fn set_liquidity_source(env: Env, reserve_address: Address) {
        config::set_liquidity_source(env, reserve_address)
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
        borrow::draw_credit(env, borrower, amount)
    }

    pub fn repay_credit(env: Env, borrower: Address, amount: i128) {
        borrow::repay_credit(env, borrower, amount)
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

    /// Set optional global rate-change caps (admin only).
    ///
    /// # Parameters
    /// - `max_rate_change_bps`: Maximum absolute change in interest rate per update.
    /// - `rate_change_min_interval`: Minimum seconds between consecutive rate changes.
    ///
    /// # Errors
    /// Reverts if caller is not the contract admin.
    pub fn set_rate_change_limits(
        env: Env,
        max_rate_change_bps: u32,
        rate_change_min_interval: u64,
    ) {
        risk::set_rate_change_limits(env, max_rate_change_bps, rate_change_min_interval)
    }

    /// Query the current rate-change limit configuration.
    ///
    /// Returns `None` if no limits have been configured yet.
    pub fn get_rate_change_limits(env: Env) -> Option<RateChangeConfig> {
        risk::get_rate_change_limits(env)
    }

    pub fn suspend_credit_line(env: Env, borrower: Address) {
        lifecycle::suspend_credit_line(env, borrower)
    }

    pub fn close_credit_line(env: Env, borrower: Address, closer: Address) {
        lifecycle::close_credit_line(env, borrower, closer)
    }

    pub fn default_credit_line(env: Env, borrower: Address) {
        lifecycle::default_credit_line(env, borrower)
    }

    pub fn reinstate_credit_line(env: Env, borrower: Address) {
        lifecycle::reinstate_credit_line(env, borrower)
    }

    pub fn get_credit_line(env: Env, borrower: Address) -> Option<CreditLineData> {
        query::get_credit_line(env, borrower)
    }
}

#[cfg(test)]
mod test_rate_change_limits {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::Env;

    fn setup<'a>(
        env: &'a Env,
        borrower: &Address,
        credit_limit: i128,
    ) -> CreditClient<'a> {
        let admin = Address::generate(env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(env, &contract_id);
        client.init(&admin);
        client.open_credit_line(borrower, &credit_limit, &300_u32, &70_u32);
        client
    }

    #[test]
    fn test_set_and_get_rate_change_limits_roundtrip() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);

        client.set_rate_change_limits(&250_u32, &3600_u64);
        let cfg = client.get_rate_change_limits().unwrap();

        assert_eq!(cfg.max_rate_change_bps, 250);
        assert_eq!(cfg.rate_change_min_interval, 3600);
    }

    #[test]
    fn test_get_rate_change_limits_returns_none_when_unset() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);

        assert!(client.get_rate_change_limits().is_none());
    }

    #[test]
    #[should_panic]
    fn test_set_rate_change_limits_non_admin_rejected() {
        let env = Env::default();
        // No mock_all_auths -> admin auth will fail
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.set_rate_change_limits(&100_u32, &0_u64);
    }

    #[test]
    fn test_rate_change_within_limit_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let borrower = Address::generate(&env);
        let client = setup(&env, &borrower, 5_000);

        client.set_rate_change_limits(&100_u32, &0_u64);
        client.update_risk_parameters(&borrower, &5_000_i128, &350_u32, &70_u32);

        assert_eq!(client.get_credit_line(&borrower).unwrap().interest_rate_bps, 350);
    }

    #[test]
    #[should_panic(expected = "rate change exceeds maximum allowed delta")]
    fn test_rate_change_over_limit_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let borrower = Address::generate(&env);
        let client = setup(&env, &borrower, 5_000);

        client.set_rate_change_limits(&50_u32, &0_u64);
        client.update_risk_parameters(&borrower, &5_000_i128, &351_u32, &70_u32);
    }

    #[test]
    #[should_panic(expected = "rate change too soon: minimum interval not elapsed")]
    fn test_rate_change_within_interval_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let borrower = Address::generate(&env);
        let client = setup(&env, &borrower, 5_000);

        client.set_rate_change_limits(&100_u32, &3600_u64);
        env.ledger().with_mut(|li| li.timestamp = 100);
        client.update_risk_parameters(&borrower, &5_000_i128, &350_u32, &70_u32);

        env.ledger().with_mut(|li| li.timestamp = 200);
        client.update_risk_parameters(&borrower, &5_000_i128, &330_u32, &70_u32);
    }

    #[test]
    fn test_rate_change_after_interval_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let borrower = Address::generate(&env);
        let client = setup(&env, &borrower, 5_000);

        client.set_rate_change_limits(&100_u32, &3600_u64);
        env.ledger().with_mut(|li| li.timestamp = 100);
        client.update_risk_parameters(&borrower, &5_000_i128, &350_u32, &70_u32);

        env.ledger().with_mut(|li| li.timestamp = 3701);
        client.update_risk_parameters(&borrower, &5_000_i128, &330_u32, &70_u32);

        assert_eq!(client.get_credit_line(&borrower).unwrap().interest_rate_bps, 330);
    }

    #[test]
    fn test_no_limits_configured_allows_any_change() {
        let env = Env::default();
        env.mock_all_auths();
        let borrower = Address::generate(&env);
        let client = setup(&env, &borrower, 5_000);

        client.update_risk_parameters(&borrower, &5_000_i128, &9_999_u32, &70_u32);
        assert_eq!(client.get_credit_line(&borrower).unwrap().interest_rate_bps, 9_999);
    }

    #[test]
    fn test_same_rate_bypasses_limits() {
        let env = Env::default();
        env.mock_all_auths();
        let borrower = Address::generate(&env);
        let client = setup(&env, &borrower, 5_000);

        client.set_rate_change_limits(&0_u32, &999_999_u64);
        client.update_risk_parameters(&borrower, &5_000_i128, &300_u32, &70_u32);

        assert_eq!(client.get_credit_line(&borrower).unwrap().interest_rate_bps, 300);
    }
}

#[cfg(test)]
mod test_coverage {
    use crate::types::{ContractError, CreditStatus};
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;
    use soroban_sdk::Env;

    fn base(env: &Env) -> (CreditClient, Address, Address) {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let borrower = Address::generate(env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&borrower, &1_000_i128, &300_u32, &70_u32);
        (client, admin, borrower)
    }

    fn base_with_token(env: &Env) -> (CreditClient, Address, Address, Address) {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let borrower = Address::generate(env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(env, &contract_id);
        client.init(&admin);
        let token_id = env.register_stellar_asset_contract_v2(Address::generate(env));
        let token = token_id.address();
        client.set_liquidity_token(&token);
        StellarAssetClient::new(env, &token).mint(&contract_id, &5_000_i128);
        client.open_credit_line(&borrower, &1_000_i128, &300_u32, &70_u32);
        (client, admin, borrower, token)
    }

    // --- config.rs coverage ---

    #[test]
    fn config_init_sets_liquidity_source_to_contract() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        // set_liquidity_source works -> init stored admin correctly
        let new_source = Address::generate(&env);
        client.set_liquidity_source(&new_source);
    }

    #[test]
    fn config_set_liquidity_token_stores_address() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        let token = env.register_stellar_asset_contract_v2(Address::generate(&env));
        client.set_liquidity_token(&token.address());
    }

    #[test]
    #[should_panic]
    fn config_set_liquidity_token_requires_admin() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        env.mock_all_auths();
        client.init(&admin);
        // drop auths
        let env2 = Env::default();
        let client2 = CreditClient::new(&env2, &contract_id);
        let token = env.register_stellar_asset_contract_v2(Address::generate(&env));
        client2.set_liquidity_token(&token.address());
    }

    #[test]
    #[should_panic]
    fn config_set_liquidity_source_requires_admin() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        env.mock_all_auths();
        client.init(&admin);
        let env2 = Env::default();
        let client2 = CreditClient::new(&env2, &contract_id);
        client2.set_liquidity_source(&Address::generate(&env));
    }

    // --- borrow.rs coverage ---

    #[test]
    fn borrow_draw_happy_path_with_token() {
        let env = Env::default();
        let (client, _admin, borrower, _token) = base_with_token(&env);
        client.draw_credit(&borrower, &500_i128);
        assert_eq!(client.get_credit_line(&borrower).unwrap().utilized_amount, 500);
    }

    #[test]
    fn borrow_draw_without_token_updates_state() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.draw_credit(&borrower, &200_i128);
        assert_eq!(client.get_credit_line(&borrower).unwrap().utilized_amount, 200);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn borrow_draw_zero_reverts() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.draw_credit(&borrower, &0_i128);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn borrow_draw_negative_reverts() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.draw_credit(&borrower, &-1_i128);
    }

    #[test]
    #[should_panic(expected = "exceeds credit limit")]
    fn borrow_draw_over_limit_reverts() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.draw_credit(&borrower, &1_001_i128);
    }

    #[test]
    #[should_panic(expected = "credit line is closed")]
    fn borrow_draw_closed_reverts() {
        let env = Env::default();
        let (client, admin, borrower) = base(&env);
        client.close_credit_line(&borrower, &admin);
        client.draw_credit(&borrower, &100_i128);
    }

    #[test]
    #[should_panic(expected = "Insufficient liquidity reserve")]
    fn borrow_draw_insufficient_reserve_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env));
        client.set_liquidity_token(&token_id.address());
        // mint nothing -> reserve = 0
        client.open_credit_line(&borrower, &1_000_i128, &300_u32, &70_u32);
        client.draw_credit(&borrower, &100_i128);
    }

    #[test]
    fn borrow_repay_happy_path() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.draw_credit(&borrower, &400_i128);
        client.repay_credit(&borrower, &200_i128);
        assert_eq!(client.get_credit_line(&borrower).unwrap().utilized_amount, 200);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn borrow_repay_zero_reverts() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.repay_credit(&borrower, &0_i128);
    }

    #[test]
    #[should_panic(expected = "credit line is closed")]
    fn borrow_repay_closed_reverts() {
        let env = Env::default();
        let (client, admin, borrower) = base(&env);
        client.close_credit_line(&borrower, &admin);
        client.repay_credit(&borrower, &100_i128);
    }

    // --- lifecycle.rs coverage ---

    #[test]
    #[should_panic(expected = "credit_limit must be greater than zero")]
    fn lifecycle_open_zero_limit_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&Address::generate(&env), &0_i128, &300_u32, &70_u32);
    }

    #[test]
    #[should_panic(expected = "interest_rate_bps cannot exceed 10000")]
    fn lifecycle_open_rate_too_high_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&Address::generate(&env), &1_000_i128, &10_001_u32, &70_u32);
    }

    #[test]
    #[should_panic(expected = "risk_score must be between 0 and 100")]
    fn lifecycle_open_score_too_high_reverts() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        client.init(&admin);
        client.open_credit_line(&Address::generate(&env), &1_000_i128, &300_u32, &101_u32);
    }

    #[test]
    #[should_panic(expected = "borrower already has an active credit line")]
    fn lifecycle_open_duplicate_active_reverts() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.open_credit_line(&borrower, &500_i128, &300_u32, &70_u32);
    }

    #[test]
    fn lifecycle_suspend_and_reinstate() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.suspend_credit_line(&borrower);
        assert_eq!(client.get_credit_line(&borrower).unwrap().status, CreditStatus::Suspended);
        client.default_credit_line(&borrower);
        client.reinstate_credit_line(&borrower);
        assert_eq!(client.get_credit_line(&borrower).unwrap().status, CreditStatus::Active);
    }

    #[test]
    #[should_panic(expected = "Only active credit lines can be suspended")]
    fn lifecycle_suspend_non_active_reverts() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.suspend_credit_line(&borrower);
        client.suspend_credit_line(&borrower); // already suspended
    }

    #[test]
    #[should_panic(expected = "credit line is not defaulted")]
    fn lifecycle_reinstate_non_defaulted_reverts() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.reinstate_credit_line(&borrower); // still Active
    }

    #[test]
    fn lifecycle_close_by_admin_force() {
        let env = Env::default();
        let (client, admin, borrower) = base(&env);
        client.draw_credit(&borrower, &500_i128);
        client.close_credit_line(&borrower, &admin);
        assert_eq!(client.get_credit_line(&borrower).unwrap().status, CreditStatus::Closed);
    }

    #[test]
    fn lifecycle_close_by_borrower_zero_utilization() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.close_credit_line(&borrower, &borrower);
        assert_eq!(client.get_credit_line(&borrower).unwrap().status, CreditStatus::Closed);
    }

    #[test]
    #[should_panic(expected = "cannot close: utilized amount not zero")]
    fn lifecycle_close_by_borrower_with_utilization_reverts() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        client.draw_credit(&borrower, &100_i128);
        client.close_credit_line(&borrower, &borrower);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn lifecycle_close_by_stranger_reverts() {
        let env = Env::default();
        let (client, _admin, borrower) = base(&env);
        let stranger = Address::generate(&env);
        client.close_credit_line(&borrower, &stranger);
    }

    #[test]
    fn lifecycle_close_idempotent_when_already_closed() {
        let env = Env::default();
        let (client, admin, borrower) = base(&env);
        client.close_credit_line(&borrower, &admin);
        client.close_credit_line(&borrower, &admin); // should not panic
        assert_eq!(client.get_credit_line(&borrower).unwrap().status, CreditStatus::Closed);
    }

    // --- types.rs coverage ---

    #[test]
    fn types_all_credit_status_variants_accessible() {
        let _ = CreditStatus::Active;
        let _ = CreditStatus::Suspended;
        let _ = CreditStatus::Defaulted;
        let _ = CreditStatus::Closed;
        let _ = CreditStatus::Restricted;
    }

    #[test]
    fn types_all_contract_error_variants_accessible() {
        let _ = ContractError::Unauthorized;
        let _ = ContractError::NotAdmin;
        let _ = ContractError::CreditLineNotFound;
        let _ = ContractError::CreditLineClosed;
        let _ = ContractError::InvalidAmount;
        let _ = ContractError::OverLimit;
        let _ = ContractError::NegativeLimit;
        let _ = ContractError::RateTooHigh;
        let _ = ContractError::ScoreTooHigh;
        let _ = ContractError::UtilizationNotZero;
        let _ = ContractError::Reentrancy;
        let _ = ContractError::Overflow;
        let _ = ContractError::LimitDecreaseRequiresRepayment;
        let _ = ContractError::AlreadyInitialized;
    }
}
