# Design Document: Duplicate Open Policy

## Overview

This design specifies the implementation of a comprehensive duplicate-open policy for the `open_credit_line` function in the Soroban credit contract. Currently, the function prevents opening a second credit line when an Active credit line exists, but behavior for non-Active statuses (Suspended, Defaulted, Closed) is undefined.

The feature extends `open_credit_line` to:
- Continue rejecting duplicate Active credit lines (preserving existing behavior)
- Allow reopening Closed, Suspended, and Defaulted credit lines with fresh parameters
- Reset `utilized_amount` and `last_rate_update_ts` to zero when reopening
- Maintain all existing input validation (credit_limit, interest_rate_bps, risk_score)
- Emit appropriate events for all state transitions

This ensures predictable behavior for backend off-chain synchronization systems and enables borrowers to receive new credit lines after their previous lines were closed, suspended, or defaulted.

## Architecture

### Current Implementation Analysis

The existing `open_credit_line` function (contracts/credit/src/lib.rs) implements:

```rust
pub fn open_credit_line(
    env: Env,
    borrower: Address,
    credit_limit: i128,
    interest_rate_bps: u32,
    risk_score: u32,
) {
    // Input validation
    assert!(credit_limit > 0, "credit_limit must be greater than zero");
    assert!(interest_rate_bps <= 10_000, "interest_rate_bps cannot exceed 10000 (100%)");
    assert!(risk_score <= 100, "risk_score must be between 0 and 100");

    // Duplicate Active check
    if let Some(existing) = env.storage().persistent().get::<Address, CreditLineData>(&borrower) {
        assert!(
            existing.status != CreditStatus::Active,
            "borrower already has an active credit line"
        );
    }
    
    // Create and store new credit line
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
    
    // Emit event
    publish_credit_line_event(...);
}
```

### Design Decision: Implicit Reopening

The current implementation already allows reopening non-Active credit lines by replacing the existing record. The key insight is that the assertion only blocks Active status, so Closed, Suspended, and Defaulted credit lines can already be overwritten.

However, the behavior is implicit and untested. This design makes it explicit, documented, and thoroughly tested.

### Modified Logic Flow


The modified `open_credit_line` function will follow this control flow:

```
1. Validate input parameters (credit_limit > 0, interest_rate_bps <= 10000, risk_score <= 100)
   ├─ If invalid: panic with appropriate error message
   └─ If valid: continue

2. Check for existing credit line
   ├─ If no existing credit line: proceed to create new credit line
   └─ If existing credit line found:
       ├─ If status == Active: panic "borrower already has an active credit line"
       └─ If status in {Closed, Suspended, Defaulted}: proceed to replace credit line

3. Create CreditLineData with:
   - borrower: provided address
   - credit_limit: provided value
   - utilized_amount: 0 (always reset)
   - interest_rate_bps: provided value
   - risk_score: provided value
   - status: Active (always set to Active)
   - last_rate_update_ts: 0 (always reset)

4. Store credit line in persistent storage

5. Emit ("credit", "opened") event with new parameters
```

### Key Design Principles

1. **Validation First**: All input validation occurs before checking existing credit lines, ensuring consistent error messages regardless of existing state.

2. **Explicit Reset**: When reopening, `utilized_amount` and `last_rate_update_ts` are explicitly set to zero, creating a clean slate for the new credit line.

3. **Status Transition**: All reopened credit lines transition to Active status, regardless of their previous status.

4. **Event Consistency**: The "opened" event is emitted for both new credit lines and reopened credit lines, maintaining consistent event semantics.

5. **Backward Compatibility**: The existing behavior for Active credit lines is preserved exactly, ensuring no breaking changes for current integrations.

## Components and Interfaces

### Modified Function Signature

No changes to the function signature:

```rust
pub fn open_credit_line(
    env: Env,
    borrower: Address,
    credit_limit: i128,
    interest_rate_bps: u32,
    risk_score: u32,
)
```

### Storage Operations

The function interacts with persistent storage:

```rust
// Read existing credit line (if any)
let existing: Option<CreditLineData> = env.storage().persistent().get(&borrower);

// Write new/updated credit line
env.storage().persistent().set(&borrower, &credit_line);
```

### Event Emission

The function emits a single event:

```rust
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
```

### Error Handling

The function uses Rust's `assert!` macro for validation errors:

| Condition | Error Message |
|-----------|---------------|
| `credit_limit <= 0` | "credit_limit must be greater than zero" |
| `interest_rate_bps > 10000` | "interest_rate_bps cannot exceed 10000 (100%)" |
| `risk_score > 100` | "risk_score must be between 0 and 100" |
| Existing Active credit line | "borrower already has an active credit line" |

All errors result in transaction reversion with no state changes.

## Data Models

### CreditLineData Structure

```rust
#[contracttype]
pub struct CreditLineData {
    pub borrower: Address,
    pub credit_limit: i128,
    pub utilized_amount: i128,
    pub interest_rate_bps: u32,
    pub risk_score: u32,
    pub status: CreditStatus,
    pub last_rate_update_ts: u64,
}
```

### Field Reset Behavior

When reopening a non-Active credit line:

| Field | Behavior |
|-------|----------|
| `borrower` | Set to provided address (unchanged) |
| `credit_limit` | Set to provided value (may change) |
| `utilized_amount` | **Reset to 0** |
| `interest_rate_bps` | Set to provided value (may change) |
| `risk_score` | Set to provided value (may change) |
| `status` | **Set to Active** |
| `last_rate_update_ts` | **Reset to 0** |

### CreditStatus Enum

```rust
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditStatus {
    Active = 0,
    Suspended = 1,
    Defaulted = 2,
    Closed = 3,
}
```

### Status Transition Matrix

| Previous Status | Can Reopen? | New Status | Notes |
|----------------|-------------|------------|-------|
| None (new borrower) | Yes | Active | Creates new credit line |
| Active | No | N/A | Panics with error |
| Suspended | Yes | Active | Replaces existing record |
| Defaulted | Yes | Active | Replaces existing record |
| Closed | Yes | Active | Replaces existing record |


## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system-essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

### Property Reflection

After analyzing all acceptance criteria, I identified several areas of redundancy:

1. **Status-specific properties (2.1-2.5, 3.1-3.5, 4.1-4.5)**: These test the same behaviors (replacement, status transition, field reset, event emission) for three different non-Active statuses. These can be combined into single properties that apply to all non-Active statuses.

2. **State preservation on failure (1.2, 5.4)**: Both test that failed operations don't modify state. These can be combined into a single invariant property.

3. **No event emission on failure (1.3, 5.5)**: Both test that failed operations don't emit events. These can be combined.

4. **Duplicate Active rejection (1.1, 8.1, 8.2)**: Requirements 8.1 and 8.2 are covered by 1.1 and 1.2.

The consolidated properties below eliminate this redundancy while maintaining complete coverage of all testable requirements.

### Property 1: Active Status Duplicate Rejection

*For any* borrower with an existing Active credit line, attempting to open a second credit line SHALL revert with the error message "borrower already has an active credit line".

**Validates: Requirements 1.1, 8.1**

### Property 2: Non-Active Status Reopening Allowed

*For any* borrower with an existing credit line in Closed, Suspended, or Defaulted status, calling open_credit_line with valid parameters SHALL succeed and replace the existing credit line with the new parameters.

**Validates: Requirements 2.1, 3.1, 4.1**

### Property 3: Reopening Transitions to Active

*For any* borrower with an existing credit line in Closed, Suspended, or Defaulted status, successfully reopening the credit line SHALL set the status to Active.

**Validates: Requirements 2.2, 3.2, 4.2**

### Property 4: Reopening Resets Utilized Amount

*For any* borrower with an existing credit line in Closed, Suspended, or Defaulted status (regardless of previous utilized_amount), successfully reopening the credit line SHALL set utilized_amount to zero.

**Validates: Requirements 2.3, 3.3, 4.3**

### Property 5: Reopening Resets Rate Update Timestamp

*For any* borrower with an existing credit line in Closed, Suspended, or Defaulted status (regardless of previous last_rate_update_ts), successfully reopening the credit line SHALL set last_rate_update_ts to zero.

**Validates: Requirements 2.5, 3.5, 4.5**

### Property 6: Reopening Emits Opened Event

*For any* borrower with an existing credit line in Closed, Suspended, or Defaulted status, successfully reopening the credit line SHALL emit exactly one ("credit", "opened") event with the new parameters.

**Validates: Requirements 2.4, 3.4, 4.4**

### Property 7: Invalid Credit Limit Rejection

*For any* credit_limit value less than or equal to zero, calling open_credit_line SHALL revert with the error message "credit_limit must be greater than zero".

**Validates: Requirements 5.1**

### Property 8: Invalid Interest Rate Rejection

*For any* interest_rate_bps value greater than 10000, calling open_credit_line SHALL revert with the error message "interest_rate_bps cannot exceed 10000 (100%)".

**Validates: Requirements 5.2**

### Property 9: Invalid Risk Score Rejection

*For any* risk_score value greater than 100, calling open_credit_line SHALL revert with the error message "risk_score must be between 0 and 100".

**Validates: Requirements 5.3**

### Property 10: Failed Operations Preserve State

*For any* existing credit line, when open_credit_line fails due to either duplicate Active status or invalid parameters, the existing credit line data SHALL remain completely unchanged.

**Validates: Requirements 1.2, 5.4, 8.2**

### Property 11: Failed Operations Emit No Events

*For any* call to open_credit_line that fails due to either duplicate Active status or invalid parameters, no ("credit", "opened") event SHALL be emitted.

**Validates: Requirements 1.3, 5.5**


## Error Handling

### Error Categories

The `open_credit_line` function handles two categories of errors:

1. **Input Validation Errors**: Triggered before any storage operations
2. **Business Logic Errors**: Triggered after reading existing state

### Error Handling Strategy

All errors use Rust's `assert!` macro, which:
- Immediately panics with the specified message
- Reverts the entire transaction
- Rolls back all state changes
- Prevents event emission

### Error Precedence

Errors are checked in this order:

1. **credit_limit validation** (checked first)
2. **interest_rate_bps validation** (checked second)
3. **risk_score validation** (checked third)
4. **Duplicate Active check** (checked last, after storage read)

This ordering ensures:
- Invalid parameters are rejected before storage operations
- Consistent error messages regardless of existing credit line state
- Minimal gas consumption for invalid inputs

### Error Messages and Conditions

| Error Message | Condition | Recovery Action |
|---------------|-----------|-----------------|
| "credit_limit must be greater than zero" | `credit_limit <= 0` | Caller must provide positive credit_limit |
| "interest_rate_bps cannot exceed 10000 (100%)" | `interest_rate_bps > 10000` | Caller must provide interest_rate_bps <= 10000 |
| "risk_score must be between 0 and 100" | `risk_score > 100` | Caller must provide risk_score <= 100 |
| "borrower already has an active credit line" | Existing Active credit line | Caller must close/suspend/default existing line first, or use different borrower address |

### Error Handling Guarantees

1. **Atomicity**: All errors result in complete transaction reversion
2. **No Partial State**: Failed operations never modify storage
3. **No Event Leakage**: Failed operations never emit events
4. **Deterministic**: Same inputs always produce same error
5. **Gas Efficiency**: Validation errors fail fast before expensive operations

### Error Testing Requirements

The test suite must verify:
- Each error condition triggers the correct error message
- Failed operations preserve existing state unchanged
- Failed operations emit no events
- Error precedence is correct (validation before business logic)

## Testing Strategy

### Dual Testing Approach

This feature requires both unit tests and property-based tests:

- **Unit tests**: Verify specific examples, edge cases, and error conditions
- **Property tests**: Verify universal properties across all inputs

Both approaches are complementary and necessary for comprehensive coverage. Unit tests catch concrete bugs and document expected behavior through examples. Property tests verify general correctness across the input space and catch edge cases that might not be obvious.

### Property-Based Testing

#### Framework Selection

Use **proptest** for Rust property-based testing. Proptest is the standard PBT library for Rust and integrates well with the existing test infrastructure.

Add to `Cargo.toml`:
```toml
[dev-dependencies]
proptest = "1.4"
```

#### Test Configuration

Each property test must:
- Run minimum 100 iterations (configured via `#[proptest(cases = 100)]`)
- Include a comment tag referencing the design property
- Use appropriate generators for test data

#### Property Test Structure

```rust
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    
    // Feature: duplicate-open-policy, Property 1: Active Status Duplicate Rejection
    #[test]
    fn prop_active_duplicate_rejection() {
        proptest!(|(
            credit_limit in 1i128..=i128::MAX,
            interest_rate_bps in 0u32..=10000,
            risk_score in 0u32..=100,
        )| {
            // Test implementation
        });
    }
}
```

### Unit Testing Strategy

#### Test Organization

Unit tests should be organized by scenario:

1. **New Credit Line Tests**
   - Opening credit line for new borrower
   - Verifying initial state

2. **Duplicate Active Tests**
   - Attempting to open duplicate Active credit line
   - Verifying error message
   - Verifying state preservation

3. **Reopening Tests** (one suite per status)
   - Reopening Closed credit line
   - Reopening Suspended credit line
   - Reopening Defaulted credit line
   - Verifying parameter replacement
   - Verifying field resets
   - Verifying status transition
   - Verifying event emission

4. **Validation Tests**
   - Invalid credit_limit (zero, negative)
   - Invalid interest_rate_bps (> 10000)
   - Invalid risk_score (> 100)
   - Verifying error messages
   - Verifying state preservation

#### Test Naming Convention

Tests should follow the pattern:
```
test_open_credit_line_{scenario}_{expected_outcome}
```

Examples:
- `test_open_credit_line_new_borrower_succeeds`
- `test_open_credit_line_duplicate_active_reverts`
- `test_open_credit_line_reopen_closed_succeeds`
- `test_open_credit_line_invalid_limit_reverts`

### Coverage Requirements

#### Line Coverage Target

Minimum 95% line coverage for the `open_credit_line` function, measured using `cargo llvm-cov`.

#### Coverage Measurement

```bash
# Install llvm-cov
cargo install cargo-llvm-cov

# Run tests with coverage
cargo llvm-cov --html

# View coverage report
open target/llvm-cov/html/index.html
```

#### Coverage Verification

The test suite must cover:
- All validation branches (3 assertions)
- Both paths of the duplicate check (existing vs. new)
- Both outcomes of the Active status check (Active vs. non-Active)
- Credit line creation and storage
- Event emission

### Test Data Generators

#### For Property Tests

```rust
// Valid credit line parameters
fn valid_credit_limit() -> impl Strategy<Value = i128> {
    1i128..=i128::MAX
}

fn valid_interest_rate() -> impl Strategy<Value = u32> {
    0u32..=10000
}

fn valid_risk_score() -> impl Strategy<Value = u32> {
    0u32..=100
}

// Invalid parameters
fn invalid_credit_limit() -> impl Strategy<Value = i128> {
    i128::MIN..=0
}

fn invalid_interest_rate() -> impl Strategy<Value = u32> {
    10001u32..=u32::MAX
}

fn invalid_risk_score() -> impl Strategy<Value = u32> {
    101u32..=u32::MAX
}

// Credit line status
fn non_active_status() -> impl Strategy<Value = CreditStatus> {
    prop_oneof![
        Just(CreditStatus::Closed),
        Just(CreditStatus::Suspended),
        Just(CreditStatus::Defaulted),
    ]
}
```

### Integration Testing

While this design focuses on unit and property tests, integration tests should verify:
- Interaction with event emission system
- Interaction with persistent storage
- End-to-end workflows (open → close → reopen)

### Test Execution

```bash
# Run all tests
cargo test

# Run only property tests
cargo test property_tests

# Run with coverage
cargo llvm-cov test

# Run specific test
cargo test test_open_credit_line_duplicate_active_reverts
```

### Expected Test Count

Minimum test coverage:

| Category | Unit Tests | Property Tests |
|----------|------------|----------------|
| New credit line | 2 | 1 |
| Duplicate Active | 3 | 1 |
| Reopen Closed | 5 | 1 |
| Reopen Suspended | 5 | 1 |
| Reopen Defaulted | 5 | 1 |
| Invalid parameters | 6 | 3 |
| State preservation | 2 | 1 |
| Event emission | 2 | 1 |
| **Total** | **30** | **11** |

This provides comprehensive coverage while avoiding redundant tests.


## Documentation Updates

### docs/credit.md Updates

The following sections in `docs/credit.md` must be updated to document the duplicate open policy:

#### 1. Update "open_credit_line" Method Documentation

**Current location**: Methods section, `open_credit_line` subsection

**Add after the parameter table**:

```markdown
#### Duplicate Open Policy

The behavior when calling `open_credit_line` for a borrower with an existing credit line depends on the current status:

| Existing Status | Behavior | Result |
|----------------|----------|--------|
| None (new borrower) | Creates new credit line | Success, status = Active |
| Active | Rejects with error | Panics: "borrower already has an active credit line" |
| Closed | Replaces existing credit line | Success, status = Active, utilized_amount = 0, last_rate_update_ts = 0 |
| Suspended | Replaces existing credit line | Success, status = Active, utilized_amount = 0, last_rate_update_ts = 0 |
| Defaulted | Replaces existing credit line | Success, status = Active, utilized_amount = 0, last_rate_update_ts = 0 |

**Reopening Behavior**: When reopening a Closed, Suspended, or Defaulted credit line:
- All parameters (credit_limit, interest_rate_bps, risk_score) are replaced with new values
- `utilized_amount` is reset to 0 (regardless of previous value)
- `last_rate_update_ts` is reset to 0 (regardless of previous value)
- `status` is set to Active
- An ("credit", "opened") event is emitted with the new parameters

**Use Case**: This allows backend systems to reopen credit lines for borrowers who have resolved previous issues (e.g., paid off defaulted debt, completed suspension period, or simply want a new credit line after closing the previous one).

**Error Handling**: Input validation occurs before checking existing credit lines, so invalid parameters will be rejected even when reopening non-Active credit lines.
```

#### 2. Update Status Transitions Table

**Current location**: Data Model section, Status transitions subsection

**Add new rows**:

```markdown
| From | To | Trigger |
|------|-----|--------|
| Closed | Active | Backend calls `open_credit_line` with new parameters (reopening). |
| Suspended | Active | Backend calls `open_credit_line` with new parameters (reopening). |
| Defaulted | Active | Backend calls `open_credit_line` with new parameters (reopening). |
```

**Add note after table**:

```markdown
**Note on Reopening**: When `open_credit_line` is called for a borrower with a Closed, Suspended, or Defaulted credit line, the existing record is completely replaced with new parameters. This is distinct from `reinstate_credit_line`, which only changes status from Defaulted to Active without modifying other parameters.
```

#### 3. Add New Section: "Duplicate Open Policy"

**Location**: Add new section after "Status transitions" and before "CreditLineEvent"

```markdown
### Duplicate Open Policy

The `open_credit_line` function enforces different policies based on the existing credit line status:

#### Active Credit Lines

Attempting to open a second credit line for a borrower with an Active credit line will fail with:
```
Error: "borrower already has an active credit line"
```

This prevents accidental overwrites of active credit lines and helps backend systems detect synchronization errors.

#### Non-Active Credit Lines (Closed, Suspended, Defaulted)

Opening a credit line for a borrower with a Closed, Suspended, or Defaulted credit line will succeed and completely replace the existing record. This enables:

1. **Closed Lines**: Borrowers can receive new credit lines after their previous lines were closed
2. **Suspended Lines**: Backend can reset credit terms without manual status transitions
3. **Defaulted Lines**: Borrowers who have resolved defaults can receive new credit lines

#### Field Reset Behavior

When reopening a non-Active credit line, the following fields are explicitly reset:

| Field | Reset Value | Rationale |
|-------|-------------|-----------|
| `utilized_amount` | 0 | New credit line starts with no utilization |
| `last_rate_update_ts` | 0 | Rate change history does not carry over |
| `status` | Active | All reopened lines become Active |

All other fields (credit_limit, interest_rate_bps, risk_score) are set to the new values provided in the function call.

#### Backend Integration Considerations

Backend systems should:
- Check existing credit line status before calling `open_credit_line`
- Use `open_credit_line` for reopening non-Active lines (simpler than close + open)
- Use `reinstate_credit_line` when only status change is needed (preserves parameters)
- Handle "borrower already has an active credit line" errors as synchronization issues
```

#### 4. Update Error Codes Table

**Current location**: Error Codes section

**No changes needed** - the existing error codes already cover the duplicate Active scenario. The error message is generated by `assert!` macro, not by a ContractError variant.

#### 5. Update Events Table

**Current location**: Events section

**Add clarification to the "opened" event row**:

```markdown
| Topic | Event Type Symbol | Emitted By | Description |
|---|---|---|---|
| `("credit", "opened")` | `opened` | `open_credit_line` | New credit line opened or existing non-Active credit line reopened |
```

### Documentation Review Checklist

Before considering documentation complete, verify:

- [ ] Duplicate open policy is clearly explained
- [ ] Status transition table includes reopening transitions
- [ ] Behavior for each status (Active, Closed, Suspended, Defaulted) is documented
- [ ] Field reset behavior is explicitly stated
- [ ] Backend integration guidance is provided
- [ ] Error messages are documented
- [ ] Event emission behavior is clarified
- [ ] Examples or CLI commands are updated if needed

### Documentation Testing

After updating documentation:

1. Review with backend team to ensure clarity
2. Verify all code examples are accurate
3. Check that status transition table is consistent with code
4. Ensure error messages match actual implementation
5. Validate that field reset behavior is correctly described


## Implementation Guidance

### Code Changes Required

#### 1. Modify open_credit_line Function

**File**: `contracts/credit/src/lib.rs`

**Current implementation** (lines ~150-200):
```rust
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
```

**Required changes**: **NONE**

The current implementation already supports the duplicate open policy correctly:
- Input validation happens first
- Active status check prevents duplicate Active credit lines
- Non-Active credit lines are implicitly allowed to be replaced
- All fields are explicitly set (utilized_amount and last_rate_update_ts are always 0)

**What needs to be done**:
1. Add comprehensive tests to verify the existing behavior
2. Update documentation to make the implicit behavior explicit
3. Achieve 95% line coverage

#### 2. Add Test Files

**File**: `contracts/credit/tests/duplicate_open_policy.rs` (new file)

Create a new test file to organize all duplicate open policy tests:

```rust
// SPDX-License-Identifier: MIT

//! Tests for duplicate open policy (issue #XX)
//!
//! Verifies that open_credit_line correctly handles:
//! - Rejecting duplicate Active credit lines
//! - Allowing reopening of Closed, Suspended, and Defaulted credit lines
//! - Resetting utilized_amount and last_rate_update_ts when reopening
//! - Validating input parameters regardless of existing status

#[cfg(test)]
mod duplicate_open_policy_tests {
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};
    use credit::{Credit, CreditClient, CreditLineData, CreditStatus};

    // Helper function to setup test environment
    fn setup() -> (Env, Address, Address, CreditClient) {
        let env = Env::default();
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        let borrower = Address::generate(&env);
        
        let contract_id = env.register(Credit, ());
        let client = CreditClient::new(&env, &contract_id);
        
        client.init(&admin);
        
        (env, admin, borrower, client)
    }

    // Unit tests go here...
}

#[cfg(test)]
mod duplicate_open_property_tests {
    use proptest::prelude::*;
    // Property tests go here...
}
```

#### 3. Update Cargo.toml

**File**: `contracts/credit/Cargo.toml`

Add proptest dependency:

```toml
[dev-dependencies]
proptest = "1.4"
```

### Implementation Steps

1. **Phase 1: Test Infrastructure** (Day 1)
   - Add proptest dependency to Cargo.toml
   - Create `tests/duplicate_open_policy.rs` file
   - Set up test helpers and generators

2. **Phase 2: Unit Tests** (Days 2-3)
   - Write unit tests for duplicate Active rejection
   - Write unit tests for reopening Closed/Suspended/Defaulted
   - Write unit tests for input validation
   - Write unit tests for state preservation and event emission

3. **Phase 3: Property Tests** (Day 4)
   - Write property tests for all 11 correctness properties
   - Configure tests to run 100+ iterations
   - Add property tags as comments

4. **Phase 4: Coverage Verification** (Day 5)
   - Run `cargo llvm-cov` to measure coverage
   - Identify any uncovered lines
   - Add tests to reach 95% coverage target

5. **Phase 5: Documentation** (Day 6)
   - Update `docs/credit.md` with duplicate open policy section
   - Update status transitions table
   - Update method documentation
   - Add backend integration guidance

6. **Phase 6: Review and Validation** (Day 7)
   - Code review with team
   - Verify all requirements are met
   - Run full test suite
   - Generate final coverage report

### Testing Commands

```bash
# Run all tests
cargo test

# Run only duplicate open policy tests
cargo test duplicate_open_policy

# Run with coverage
cargo llvm-cov test

# Generate HTML coverage report
cargo llvm-cov --html --open

# Run property tests with verbose output
cargo test property_tests -- --nocapture

# Run specific test
cargo test test_open_credit_line_reopen_closed_succeeds
```

### Verification Checklist

Before marking implementation complete:

- [ ] All 11 correctness properties have corresponding property tests
- [ ] All property tests run 100+ iterations
- [ ] All property tests include comment tags
- [ ] Minimum 30 unit tests implemented
- [ ] 95% line coverage achieved for open_credit_line
- [ ] All tests pass
- [ ] Documentation updated in docs/credit.md
- [ ] Status transitions table updated
- [ ] Backend integration guidance added
- [ ] Code review completed
- [ ] No breaking changes to existing behavior

### Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Breaking existing behavior | Preserve existing test `test_open_credit_line_duplicate_active_borrower_reverts` |
| Insufficient test coverage | Use cargo llvm-cov to measure and verify 95% coverage |
| Property tests too slow | Limit to 100 iterations per test (configurable) |
| Unclear documentation | Review with backend team before finalizing |
| Missing edge cases | Use property-based testing to explore input space |

## Summary

This design specifies the implementation of a comprehensive duplicate open policy for the `open_credit_line` function. The key insight is that the current implementation already supports the desired behavior - it just needs to be thoroughly tested and documented.

### Key Design Decisions

1. **No Code Changes Required**: The existing implementation already handles reopening correctly by checking only for Active status duplicates.

2. **Explicit Field Reset**: When reopening, `utilized_amount` and `last_rate_update_ts` are always set to 0, creating a clean slate.

3. **Validation First**: Input validation occurs before checking existing credit lines, ensuring consistent error messages.

4. **Property-Based Testing**: Using proptest with 100+ iterations per property to verify correctness across the input space.

5. **Comprehensive Documentation**: Making the implicit reopening behavior explicit in docs/credit.md.

### Implementation Effort

- **Code changes**: Minimal (no changes to open_credit_line function)
- **Test implementation**: ~40 tests (30 unit + 11 property)
- **Documentation updates**: 5 sections in docs/credit.md
- **Estimated timeline**: 7 days

### Success Criteria

1. All 11 correctness properties verified by property tests
2. Minimum 95% line coverage for open_credit_line function
3. All existing tests continue to pass
4. Documentation clearly explains duplicate open policy
5. Backend team confirms documentation is clear and accurate

### Next Steps

1. Create task list from this design
2. Implement test infrastructure (proptest setup)
3. Write unit tests for all scenarios
4. Write property tests for all properties
5. Verify coverage meets 95% target
6. Update documentation
7. Conduct code review
8. Merge to main branch

