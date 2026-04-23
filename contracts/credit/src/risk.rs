use crate::auth::require_admin_auth;
use crate::events::{
    publish_rate_formula_config_event, publish_risk_parameters_updated,
    RateFormulaConfigEvent, RiskParametersUpdatedEvent,
};
use crate::storage::{rate_cfg_key, rate_formula_key};
use crate::types::{CreditLineData, RateChangeConfig, RateFormulaConfig};
use soroban_sdk::{Address, Env};

/// Maximum interest rate in basis points (100%).
pub const MAX_INTEREST_RATE_BPS: u32 = 10_000;

/// Maximum risk score (0–100 scale).
pub const MAX_RISK_SCORE: u32 = 100;

/// Compute interest rate from risk score using piecewise-linear formula.
///
/// # Formula
/// ```text
/// raw_rate = base_rate_bps + (risk_score * slope_bps_per_score)
/// effective_rate = clamp(raw_rate, min_rate_bps, min(max_rate_bps, MAX_INTEREST_RATE_BPS))
/// ```
///
/// Uses saturating arithmetic to prevent overflow — if the multiplication
/// overflows u32, it saturates to `u32::MAX` and is then clamped by the
/// upper bound.
///
/// # Arguments
/// * `cfg` — The rate formula configuration.
/// * `risk_score` — The borrower's risk score (0–100).
///
/// # Returns
/// The computed effective interest rate in basis points.
pub fn compute_rate_from_score(cfg: &RateFormulaConfig, risk_score: u32) -> u32 {
    let raw = cfg
        .base_rate_bps
        .saturating_add(risk_score.saturating_mul(cfg.slope_bps_per_score));
    let upper = cfg.max_rate_bps.min(MAX_INTEREST_RATE_BPS);
    raw.clamp(cfg.min_rate_bps, upper)
}

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
    if risk_score > MAX_RISK_SCORE {
        panic!("risk_score exceeds maximum");
    }

    // Determine the effective interest rate:
    // - If a rate formula config is stored, compute from risk_score (ignore passed rate).
    // - Otherwise, use the manually supplied interest_rate_bps (existing behavior).
    let effective_rate = if let Some(formula_cfg) = env
        .storage()
        .instance()
        .get::<_, RateFormulaConfig>(&rate_formula_key(&env))
    {
        compute_rate_from_score(&formula_cfg, risk_score)
    } else {
        interest_rate_bps
    };

    if effective_rate > MAX_INTEREST_RATE_BPS {
        panic!("interest_rate_bps exceeds maximum");
    }

    if effective_rate != credit_line.interest_rate_bps {
        if let Some(cfg) = env
            .storage()
            .instance()
            .get::<_, RateChangeConfig>(&rate_cfg_key(&env))
        {
            let old_rate = credit_line.interest_rate_bps;
            let delta = effective_rate.abs_diff(old_rate);

            if delta > cfg.max_rate_change_bps {
                panic!("rate change exceeds maximum allowed delta");
            }

            if cfg.rate_change_min_interval > 0 && credit_line.last_rate_update_ts != 0 {
                let now = env.ledger().timestamp();
                let elapsed = now.saturating_sub(credit_line.last_rate_update_ts);
                if elapsed < cfg.rate_change_min_interval {
                    panic!("rate change too soon: minimum interval not elapsed");
                }
            }
        }

        credit_line.last_rate_update_ts = env.ledger().timestamp();
    }

    credit_line.credit_limit = credit_limit;
    credit_line.interest_rate_bps = effective_rate;
    credit_line.risk_score = risk_score;
    env.storage().persistent().set(&borrower, &credit_line);

    publish_risk_parameters_updated(
        &env,
        RiskParametersUpdatedEvent {
            borrower: borrower.clone(),
            credit_limit,
            interest_rate_bps: effective_rate,
            risk_score,
        },
    );
}

/// Set the risk-score-based rate formula configuration (admin only).
///
/// Once set, `update_risk_parameters` will automatically compute
/// `interest_rate_bps` from the borrower's `risk_score` using the
/// piecewise-linear formula, ignoring the manually supplied rate.
///
/// # Validation
/// - `min_rate_bps` must be ≤ `max_rate_bps`.
/// - `max_rate_bps` must be ≤ `MAX_INTEREST_RATE_BPS` (10,000).
/// - `base_rate_bps` must be ≤ `MAX_INTEREST_RATE_BPS`.
///
/// # Events
/// Emits `("credit", "rate_cfg")` with `RateFormulaConfigEvent { enabled: true, ... }`.
pub fn set_rate_formula_config(
    env: Env,
    base_rate_bps: u32,
    slope_bps_per_score: u32,
    min_rate_bps: u32,
    max_rate_bps: u32,
) {
    require_admin_auth(&env);

    if min_rate_bps > max_rate_bps {
        panic!("min_rate_bps must be <= max_rate_bps");
    }
    if max_rate_bps > MAX_INTEREST_RATE_BPS {
        panic!("max_rate_bps exceeds MAX_INTEREST_RATE_BPS");
    }
    if base_rate_bps > MAX_INTEREST_RATE_BPS {
        panic!("base_rate_bps exceeds MAX_INTEREST_RATE_BPS");
    }

    let cfg = RateFormulaConfig {
        base_rate_bps,
        slope_bps_per_score,
        min_rate_bps,
        max_rate_bps,
    };
    env.storage()
        .instance()
        .set(&rate_formula_key(&env), &cfg);

    publish_rate_formula_config_event(
        &env,
        RateFormulaConfigEvent {
            base_rate_bps,
            slope_bps_per_score,
            min_rate_bps,
            max_rate_bps,
            enabled: true,
        },
    );
}

/// Remove the rate formula configuration, reverting to manual rate mode (admin only).
///
/// After this call, `update_risk_parameters` will use the manually supplied
/// `interest_rate_bps` as before.
///
/// # Events
/// Emits `("credit", "rate_cfg")` with `RateFormulaConfigEvent { enabled: false, ... }`.
pub fn clear_rate_formula_config(env: Env) {
    require_admin_auth(&env);
    env.storage()
        .instance()
        .remove(&rate_formula_key(&env));

    publish_rate_formula_config_event(
        &env,
        RateFormulaConfigEvent {
            base_rate_bps: 0,
            slope_bps_per_score: 0,
            min_rate_bps: 0,
            max_rate_bps: 0,
            enabled: false,
        },
    );
}

/// Get the current rate formula configuration (view function).
///
/// Returns `None` if no formula is configured (manual mode).
pub fn get_rate_formula_config(env: Env) -> Option<RateFormulaConfig> {
    env.storage()
        .instance()
        .get(&rate_formula_key(&env))
}

/// Set rate-change limits (admin only).
///
/// Configures the maximum allowed interest-rate change per call and the
/// minimum time interval between consecutive rate changes.
pub fn set_rate_change_limits(env: Env, max_rate_change_bps: u32, rate_change_min_interval: u64) {
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
