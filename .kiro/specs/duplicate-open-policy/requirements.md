# Requirements Document

## Introduction

This document specifies the behavior of the `open_credit_line` function when invoked multiple times for the same borrower. The current implementation prevents opening a second credit line when an Active credit line exists, but the behavior for non-Active statuses (Suspended, Defaulted, Closed) is undefined. This feature defines and tests the complete duplicate-open policy to ensure predictable behavior for backend off-chain synchronization systems.

## Glossary

- **Credit_Contract**: The Soroban smart contract that manages credit lines for borrowers
- **Borrower**: A Stellar address that has or will have a credit line
- **Credit_Line**: A persistent storage record containing borrower credit parameters and status
- **Active_Status**: CreditStatus::Active (value 0) - credit line is open and available
- **Suspended_Status**: CreditStatus::Suspended (value 1) - credit line is temporarily suspended
- **Defaulted_Status**: CreditStatus::Defaulted (value 2) - borrower has defaulted
- **Closed_Status**: CreditStatus::Closed (value 3) - credit line has been closed
- **Backend_System**: The off-chain risk engine or service that calls open_credit_line
- **Duplicate_Open**: A second invocation of open_credit_line for a borrower that already has a credit line record

## Requirements

### Requirement 1: Prevent Duplicate Active Credit Lines

**User Story:** As a backend system operator, I want duplicate open_credit_line calls to fail when an Active credit line exists, so that I can detect synchronization errors and prevent data corruption.

#### Acceptance Criteria

1. WHEN open_credit_line is called for a Borrower with an existing Active_Status credit line, THE Credit_Contract SHALL revert with the error message "borrower already has an active credit line"
2. WHEN open_credit_line reverts due to duplicate Active_Status, THE Credit_Contract SHALL preserve the existing Credit_Line data unchanged
3. WHEN open_credit_line reverts due to duplicate Active_Status, THE Credit_Contract SHALL NOT emit an opened event

### Requirement 2: Allow Reopening Closed Credit Lines

**User Story:** As a backend system operator, I want to reopen a Closed credit line with new parameters, so that borrowers can receive new credit lines after their previous lines were closed.

#### Acceptance Criteria

1. WHEN open_credit_line is called for a Borrower with an existing Closed_Status credit line, THE Credit_Contract SHALL replace the existing Credit_Line with the new parameters
2. WHEN open_credit_line succeeds for a Closed_Status credit line, THE Credit_Contract SHALL set the status to Active_Status
3. WHEN open_credit_line succeeds for a Closed_Status credit line, THE Credit_Contract SHALL set utilized_amount to zero
4. WHEN open_credit_line succeeds for a Closed_Status credit line, THE Credit_Contract SHALL emit an opened event with the new parameters
5. WHEN open_credit_line succeeds for a Closed_Status credit line, THE Credit_Contract SHALL set last_rate_update_ts to zero

### Requirement 3: Allow Reopening Suspended Credit Lines

**User Story:** As a backend system operator, I want to reopen a Suspended credit line with new parameters, so that I can reset credit terms without requiring manual status transitions.

#### Acceptance Criteria

1. WHEN open_credit_line is called for a Borrower with an existing Suspended_Status credit line, THE Credit_Contract SHALL replace the existing Credit_Line with the new parameters
2. WHEN open_credit_line succeeds for a Suspended_Status credit line, THE Credit_Contract SHALL set the status to Active_Status
3. WHEN open_credit_line succeeds for a Suspended_Status credit line, THE Credit_Contract SHALL set utilized_amount to zero
4. WHEN open_credit_line succeeds for a Suspended_Status credit line, THE Credit_Contract SHALL emit an opened event with the new parameters
5. WHEN open_credit_line succeeds for a Suspended_Status credit line, THE Credit_Contract SHALL set last_rate_update_ts to zero

### Requirement 4: Allow Reopening Defaulted Credit Lines

**User Story:** As a backend system operator, I want to reopen a Defaulted credit line with new parameters, so that borrowers who have resolved their defaults can receive new credit lines.

#### Acceptance Criteria

1. WHEN open_credit_line is called for a Borrower with an existing Defaulted_Status credit line, THE Credit_Contract SHALL replace the existing Credit_Line with the new parameters
2. WHEN open_credit_line succeeds for a Defaulted_Status credit line, THE Credit_Contract SHALL set the status to Active_Status
3. WHEN open_credit_line succeeds for a Defaulted_Status credit line, THE Credit_Contract SHALL set utilized_amount to zero
4. WHEN open_credit_line succeeds for a Defaulted_Status credit line, THE Credit_Contract SHALL emit an opened event with the new parameters
5. WHEN open_credit_line succeeds for a Defaulted_Status credit line, THE Credit_Contract SHALL set last_rate_update_ts to zero

### Requirement 5: Validate Input Parameters on Duplicate Open

**User Story:** As a backend system operator, I want duplicate open_credit_line calls to validate input parameters, so that invalid parameters are rejected even when reopening non-Active credit lines.

#### Acceptance Criteria

1. WHEN open_credit_line is called with credit_limit less than or equal to zero, THE Credit_Contract SHALL revert with the error message "credit_limit must be greater than zero"
2. WHEN open_credit_line is called with interest_rate_bps greater than 10000, THE Credit_Contract SHALL revert with the error message "interest_rate_bps cannot exceed 10000 (100%)"
3. WHEN open_credit_line is called with risk_score greater than 100, THE Credit_Contract SHALL revert with the error message "risk_score must be between 0 and 100"
4. WHEN open_credit_line reverts due to invalid parameters, THE Credit_Contract SHALL preserve the existing Credit_Line data unchanged
5. WHEN open_credit_line reverts due to invalid parameters, THE Credit_Contract SHALL NOT emit an opened event

### Requirement 6: Document Duplicate Open Policy

**User Story:** As a developer integrating with the Credit contract, I want the duplicate open policy documented in docs/credit.md, so that I understand the expected behavior when calling open_credit_line multiple times.

#### Acceptance Criteria

1. THE Credit_Contract documentation SHALL describe the behavior when open_credit_line is called for a Borrower with an existing Active_Status credit line
2. THE Credit_Contract documentation SHALL describe the behavior when open_credit_line is called for a Borrower with an existing Closed_Status credit line
3. THE Credit_Contract documentation SHALL describe the behavior when open_credit_line is called for a Borrower with an existing Suspended_Status credit line
4. THE Credit_Contract documentation SHALL describe the behavior when open_credit_line is called for a Borrower with an existing Defaulted_Status credit line
5. THE Credit_Contract documentation SHALL include a table or section summarizing the duplicate open policy for all status values

### Requirement 7: Achieve Minimum Test Coverage

**User Story:** As a maintainer, I want minimum 95% line coverage for the duplicate open policy implementation, so that the behavior is thoroughly tested and regressions are prevented.

#### Acceptance Criteria

1. WHEN cargo llvm-cov is executed, THE test suite SHALL achieve at least 95% line coverage for the open_credit_line function
2. THE test suite SHALL include tests for duplicate open with Active_Status that verify revert behavior
3. THE test suite SHALL include tests for duplicate open with Closed_Status that verify replacement behavior
4. THE test suite SHALL include tests for duplicate open with Suspended_Status that verify replacement behavior
5. THE test suite SHALL include tests for duplicate open with Defaulted_Status that verify replacement behavior
6. THE test suite SHALL include tests for duplicate open with invalid parameters that verify validation behavior
7. THE test suite SHALL include tests that verify utilized_amount is reset to zero when reopening non-Active credit lines
8. THE test suite SHALL include tests that verify last_rate_update_ts is reset to zero when reopening non-Active credit lines

### Requirement 8: Preserve Idempotency for Active Status

**User Story:** As a backend system operator, I want the Active status duplicate-open behavior to remain unchanged, so that existing integrations continue to work correctly.

#### Acceptance Criteria

1. WHEN open_credit_line is called for a Borrower with an existing Active_Status credit line, THE Credit_Contract SHALL maintain the existing revert behavior
2. THE Credit_Contract SHALL NOT introduce any new side effects when reverting duplicate Active_Status opens
3. THE existing test test_open_credit_line_duplicate_active_borrower_reverts SHALL continue to pass without modification
