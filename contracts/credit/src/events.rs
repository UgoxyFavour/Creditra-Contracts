// SPDX-License-Identifier: MIT
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![cfg_attr(coverage_nightly, coverage(off))]

//! Event types and topic constants for the Credit contract.
//! Stable event schemas for indexing and analytics.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

use crate::types::CreditStatus;

/// Event emitted when a credit line lifecycle event occurs (opened, suspend, closed, default).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreditLineEvent {
    pub event_type: Symbol,
    pub borrower: Address,
    pub status: CreditStatus,
    pub credit_limit: i128,
    pub interest_rate_bps: u32,
    pub risk_score: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreditLineEventV2 {
    pub event_type: Symbol,
    pub borrower: Address,
    pub status: CreditStatus,
    pub credit_limit: i128,
    pub interest_rate_bps: u32,
    pub risk_score: u32,
    pub timestamp: u64,
    pub actor: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepaymentEvent {
    pub borrower: Address,
    pub amount: i128,
    pub interest_repaid: i128,
    pub principal_repaid: i128,
    pub new_utilized_amount: i128,
    pub new_accrued_interest: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepaymentEventV2 {
    pub borrower: Address,
    pub payer: Address,
    pub amount: i128,
    pub interest_repaid: i128,
    pub principal_repaid: i128,
    pub new_utilized_amount: i128,
    pub new_accrued_interest: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskParametersUpdatedEvent {
    pub borrower: Address,
    pub credit_limit: i128,
    pub interest_rate_bps: u32,
    pub risk_score: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskParametersUpdatedEventV2 {
    pub borrower: Address,
    pub credit_limit: i128,
    pub interest_rate_bps: u32,
    pub risk_score: u32,
    pub timestamp: u64,
    pub actor: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrawnEvent {
    pub borrower: Address,
    pub amount: i128,
    pub new_utilized_amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrawReversedEvent {
    pub borrower: Address,
    pub amount: i128,
    pub original_ts: u64,
    pub reason_code: u32,
    pub new_utilized_amount: i128,
    pub timestamp: u64,
    pub admin: Address,
    pub accounting_only: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterestAccruedEvent {
    pub borrower: Address,
    pub accrued_amount: i128,
    pub total_accrued_interest: i128,
    pub new_utilized_amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrawsFrozenEvent {
    pub frozen: bool,
    pub timestamp: u64,
    pub actor: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrawnEventV2 {
    pub borrower: Address,
    pub recipient: Address,
    pub reserve_source: Address,
    pub amount: i128,
    pub new_utilized_amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminRotationProposedEvent {
    pub current_admin: Address,
    pub proposed_admin: Address,
    pub accept_after: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminRotationAcceptedEvent {
    pub previous_admin: Address,
    pub new_admin: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorrowerBlockedEvent {
    pub borrower: Address,
    pub blocked: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateFormulaConfigEvent {
    pub enabled: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefaultLiquidationRequestedEvent {
    pub borrower: Address,
    pub utilized_amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefaultLiquidationSettledEvent {
    pub borrower: Address,
    pub settlement_id: Symbol,
    pub recovered_amount: i128,
    pub remaining_utilized_amount: i128,
    pub status: CreditStatus,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PausedEvent {
    pub paused: bool,
    pub timestamp: u64,
    pub actor: Address,
}

pub fn publish_credit_line_event(env: &Env, topic: (Symbol, Symbol), event: CreditLineEvent) {
    env.events().publish(topic, event);
}

#[allow(dead_code)]
pub fn publish_credit_line_event_v2(env: &Env, topic: (Symbol, Symbol), event: CreditLineEventV2) {
    env.events().publish(topic, event);
}

pub fn publish_repayment_event(env: &Env, event: RepaymentEvent) {
    env.events()
        .publish((symbol_short!("credit"), symbol_short!("repay")), event);
}

#[allow(dead_code)]
pub fn publish_repayment_event_v2(env: &Env, event: RepaymentEventV2) {
    env.events().publish(
        (symbol_short!("credit"), Symbol::new(env, "repay_v2")),
        event,
    );
}

pub fn publish_drawn_event(env: &Env, event: DrawnEvent) {
    env.events()
        .publish((symbol_short!("credit"), symbol_short!("drawn")), event);
}

#[allow(dead_code)]
pub fn publish_draw_reversed_event(env: &Env, event: DrawReversedEvent) {
    env.events()
        .publish((symbol_short!("credit"), symbol_short!("draw_rev")), event);
}

#[allow(dead_code)]
pub fn publish_drawn_event_v2(env: &Env, event: DrawnEventV2) {
    env.events()
        .publish((symbol_short!("credit"), symbol_short!("drawn_v2")), event);
}

pub fn publish_admin_rotation_proposed(env: &Env, event: AdminRotationProposedEvent) {
    env.events().publish(
        (symbol_short!("credit"), Symbol::new(env, "admin_prop")),
        event,
    );
}

pub fn publish_admin_rotation_accepted(env: &Env, event: AdminRotationAcceptedEvent) {
    env.events().publish(
        (symbol_short!("credit"), Symbol::new(env, "admin_acc")),
        event,
    );
}

pub fn publish_risk_parameters_updated(env: &Env, event: RiskParametersUpdatedEvent) {
    env.events()
        .publish((symbol_short!("credit"), symbol_short!("risk_upd")), event);
}

#[allow(dead_code)]
pub fn publish_interest_accrued_event(env: &Env, event: InterestAccruedEvent) {
    env.events()
        .publish((symbol_short!("credit"), symbol_short!("accrue")), event);
}

pub fn publish_draws_frozen_event(env: &Env, event: DrawsFrozenEvent) {
    env.events().publish(
        (symbol_short!("credit"), Symbol::new(env, "drw_freeze")),
        event,
    );
}

#[allow(dead_code)]
pub fn publish_borrower_blocked_event(env: &Env, event: BorrowerBlockedEvent) {
    env.events()
        .publish((symbol_short!("credit"), symbol_short!("blk_chg")), event);
}

pub fn publish_rate_formula_config_event(env: &Env, event: RateFormulaConfigEvent) {
    env.events().publish(
        (symbol_short!("credit"), Symbol::new(env, "rate_form")),
        event,
    );
}

pub fn publish_default_liquidation_requested_event(
    env: &Env,
    event: DefaultLiquidationRequestedEvent,
) {
    env.events().publish(
        (symbol_short!("credit"), Symbol::new(env, "liq_req")),
        event,
    );
}

pub fn publish_default_liquidation_settled_event(
    env: &Env,
    event: DefaultLiquidationSettledEvent,
) {
    env.events().publish(
        (symbol_short!("credit"), Symbol::new(env, "liq_set")),
        event,
    );
}

pub fn publish_paused_event(env: &Env, event: PausedEvent) {
    let topic = if event.paused {
        Symbol::new(env, "paused")
    } else {
        Symbol::new(env, "unpaused")
    };
    env.events()
        .publish((symbol_short!("credit"), topic), event);
}
