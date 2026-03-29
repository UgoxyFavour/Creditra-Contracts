# Implementation Plan: Duplicate Open Policy

## Overview

This implementation plan focuses on comprehensive testing and documentation for the duplicate open policy in the `open_credit_line` function. The existing implementation already supports the desired behavior (rejecting duplicate Active credit lines while allowing reopening of Closed, Suspended, and Defaulted credit lines). This plan adds thorough test coverage and explicit documentation to make the implicit behavior explicit and verifiable.

## Tasks

- [x] 1. Set up test infrastructure
  - Add proptest dependency to contracts/credit/Cargo.toml
  - Create contracts/credit/tests/duplicate_open_policy.rs test file
  - Set up test helper functions and generators
  - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6, 7.7, 7.8_

- [x] 2. Implement unit tests for duplicate Active rejection
  - [x] 2.1 Write test for duplicate Active credit line rejection
    - Test that opening a second credit line for a borrower with Active status fails with "borrower already has an active credit line"
    - _Requirements: 1.1, 8.1_
  
  - [x] 2.2 Write test for state preservation on duplicate Active rejection
    - Test that failed duplicate Active open preserves existing credit line data unchanged
    - _Requirements: 1.2, 8.2_
  
  - [x] 2.3 Write test for no event emission on duplicate Active rejection
    - Test that failed duplicate Active open does not emit an opened event
    - _Requirements: 1.3_

- [x] 3. Implement unit tests for reopening Closed credit lines
  - [x] 3.1 Write test for reopening Closed credit line with new parameters
    - Test that opening a credit line for a borrower with Closed status succeeds and replaces parameters
    - _Requirements: 2.1_
  
  - [x] 3.2 Write test for Closed to Active status transition
    - Test that reopening a Closed credit line sets status to Active
    - _Requirements: 2.2_
  
  - [x] 3.3 Write test for utilized_amount reset on Closed reopening
    - Test that reopening a Closed credit line sets utilized_amount to zero
    - _Requirements: 2.3, 7.7_
  
  - [x] 3.4 Write test for event emission on Closed reopening
    - Test that reopening a Closed credit line emits an opened event with new parameters
    - _Requirements: 2.4_
  
  - [x] 3.5 Write test for last_rate_update_ts reset on Closed reopening
    - Test that reopening a Closed credit line sets last_rate_update_ts to zero
    - _Requirements: 2.5, 7.8_

- [x] 4. Implement unit tests for reopening Suspended credit lines
  - [x] 4.1 Write test for reopening Suspended credit line with new parameters
    - Test that opening a credit line for a borrower with Suspended status succeeds and replaces parameters
    - _Requirements: 3.1_
  
  - [x] 4.2 Write test for Suspended to Active status transition
    - Test that reopening a Suspended credit line sets status to Active
    - _Requirements: 3.2_
  
  - [x] 4.3 Write test for utilized_amount reset on Suspended reopening
    - Test that reopening a Suspended credit line sets utilized_amount to zero
    - _Requirements: 3.3, 7.7_
  
  - [x] 4.4 Write test for event emission on Suspended reopening
    - Test that reopening a Suspended credit line emits an opened event with new parameters
    - _Requirements: 3.4_
  
  - [x] 4.5 Write test for last_rate_update_ts reset on Suspended reopening
    - Test that reopening a Suspended credit line sets last_rate_update_ts to zero
    - _Requirements: 3.5, 7.8_

- [x] 5. Implement unit tests for reopening Defaulted credit lines
  - [x] 5.1 Write test for reopening Defaulted credit line with new parameters
    - Test that opening a credit line for a borrower with Defaulted status succeeds and replaces parameters
    - _Requirements: 4.1_
  
  - [x] 5.2 Write test for Defaulted to Active status transition
    - Test that reopening a Defaulted credit line sets status to Active
    - _Requirements: 4.2_
  
  - [x] 5.3 Write test for utilized_amount reset on Defaulted reopening
    - Test that reopening a Defaulted credit line sets utilized_amount to zero
    - _Requirements: 4.3, 7.7_
  
  - [x] 5.4 Write test for event emission on Defaulted reopening
    - Test that reopening a Defaulted credit line emits an opened event with new parameters
    - _Requirements: 4.4_
  
  - [x] 5.5 Write test for last_rate_update_ts reset on Defaulted reopening
    - Test that reopening a Defaulted credit line sets last_rate_update_ts to zero
    - _Requirements: 4.5, 7.8_

- [x] 6. Implement unit tests for input validation
  - [x] 6.1 Write test for zero credit_limit rejection
    - Test that opening a credit line with credit_limit = 0 fails with "credit_limit must be greater than zero"
    - _Requirements: 5.1_
  
  - [x] 6.2 Write test for negative credit_limit rejection
    - Test that opening a credit line with credit_limit < 0 fails with "credit_limit must be greater than zero"
    - _Requirements: 5.1_
  
  - [x] 6.3 Write test for excessive interest_rate_bps rejection
    - Test that opening a credit line with interest_rate_bps > 10000 fails with "interest_rate_bps cannot exceed 10000 (100%)"
    - _Requirements: 5.2_
  
  - [x] 6.4 Write test for excessive risk_score rejection
    - Test that opening a credit line with risk_score > 100 fails with "risk_score must be between 0 and 100"
    - _Requirements: 5.3_
  
  - [x] 6.5 Write test for state preservation on validation failure
    - Test that failed validation preserves existing credit line data unchanged
    - _Requirements: 5.4_
  
  - [x] 6.6 Write test for no event emission on validation failure
    - Test that failed validation does not emit an opened event
    - _Requirements: 5.5_

- [x] 7. Implement unit tests for edge cases
  - [x] 7.1 Write test for reopening with different parameters
    - Test that reopening replaces all parameters (credit_limit, interest_rate_bps, risk_score)
    - _Requirements: 2.1, 3.1, 4.1_
  
  - [x] 7.2 Write test for reopening with non-zero utilized_amount
    - Test that reopening resets utilized_amount to zero even when previous value was non-zero
    - _Requirements: 2.3, 3.3, 4.3, 7.7_
  
  - [x] 7.3 Write test for reopening with non-zero last_rate_update_ts
    - Test that reopening resets last_rate_update_ts to zero even when previous value was non-zero
    - _Requirements: 2.5, 3.5, 4.5, 7.8_

- [ ]* 8. Implement property tests for all correctness properties
  - [ ]* 8.1 Write property test for Active status duplicate rejection
    - **Property 1: Active Status Duplicate Rejection**
    - **Validates: Requirements 1.1, 8.1**
    - Test that for any borrower with Active credit line, duplicate open fails
    - Configure with 100+ iterations
    - _Requirements: 7.2_
  
  - [ ]* 8.2 Write property test for non-Active status reopening
    - **Property 2: Non-Active Status Reopening Allowed**
    - **Validates: Requirements 2.1, 3.1, 4.1**
    - Test that for any borrower with Closed/Suspended/Defaulted credit line, reopening succeeds
    - Configure with 100+ iterations
    - _Requirements: 7.3, 7.4, 7.5_
  
  - [ ]* 8.3 Write property test for reopening transitions to Active
    - **Property 3: Reopening Transitions to Active**
    - **Validates: Requirements 2.2, 3.2, 4.2**
    - Test that reopening any non-Active credit line sets status to Active
    - Configure with 100+ iterations
    - _Requirements: 7.3, 7.4, 7.5_
  
  - [ ]* 8.4 Write property test for reopening resets utilized_amount
    - **Property 4: Reopening Resets Utilized Amount**
    - **Validates: Requirements 2.3, 3.3, 4.3**
    - Test that reopening any non-Active credit line sets utilized_amount to zero
    - Configure with 100+ iterations
    - _Requirements: 7.7_
  
  - [ ]* 8.5 Write property test for reopening resets last_rate_update_ts
    - **Property 5: Reopening Resets Rate Update Timestamp**
    - **Validates: Requirements 2.5, 3.5, 4.5**
    - Test that reopening any non-Active credit line sets last_rate_update_ts to zero
    - Configure with 100+ iterations
    - _Requirements: 7.8_
  
  - [ ]* 8.6 Write property test for reopening emits opened event
    - **Property 6: Reopening Emits Opened Event**
    - **Validates: Requirements 2.4, 3.4, 4.4**
    - Test that reopening any non-Active credit line emits exactly one opened event
    - Configure with 100+ iterations
    - _Requirements: 7.3, 7.4, 7.5_
  
  - [ ]* 8.7 Write property test for invalid credit_limit rejection
    - **Property 7: Invalid Credit Limit Rejection**
    - **Validates: Requirements 5.1**
    - Test that any credit_limit <= 0 is rejected
    - Configure with 100+ iterations
    - _Requirements: 7.6_
  
  - [ ]* 8.8 Write property test for invalid interest_rate_bps rejection
    - **Property 8: Invalid Interest Rate Rejection**
    - **Validates: Requirements 5.2**
    - Test that any interest_rate_bps > 10000 is rejected
    - Configure with 100+ iterations
    - _Requirements: 7.6_
  
  - [ ]* 8.9 Write property test for invalid risk_score rejection
    - **Property 9: Invalid Risk Score Rejection**
    - **Validates: Requirements 5.3**
    - Test that any risk_score > 100 is rejected
    - Configure with 100+ iterations
    - _Requirements: 7.6_
  
  - [ ]* 8.10 Write property test for failed operations preserve state
    - **Property 10: Failed Operations Preserve State**
    - **Validates: Requirements 1.2, 5.4, 8.2**
    - Test that any failed open_credit_line call preserves existing credit line unchanged
    - Configure with 100+ iterations
    - _Requirements: 7.2, 7.6_
  
  - [ ]* 8.11 Write property test for failed operations emit no events
    - **Property 11: Failed Operations Emit No Events**
    - **Validates: Requirements 1.3, 5.5**
    - Test that any failed open_credit_line call emits no events
    - Configure with 100+ iterations
    - _Requirements: 7.2, 7.6_

- [x] 9. Verify test coverage meets 95% target
  - Run cargo llvm-cov to measure line coverage for open_credit_line function
  - Identify any uncovered lines
  - Add additional tests if coverage is below 95%
  - Generate HTML coverage report
  - _Requirements: 7.1_

- [x] 10. Checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [x] 11. Update docs/credit.md with duplicate open policy documentation
  - [x] 11.1 Update open_credit_line method documentation
    - Add "Duplicate Open Policy" subsection with status behavior table
    - Document reopening behavior and field reset rules
    - Add use case explanation and error handling notes
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5_
  
  - [x] 11.2 Update status transitions table
    - Add rows for Closed → Active, Suspended → Active, Defaulted → Active transitions
    - Add note distinguishing reopening from reinstate_credit_line
    - _Requirements: 6.2, 6.3, 6.4_
  
  - [x] 11.3 Add new "Duplicate Open Policy" section
    - Document Active credit line rejection policy
    - Document non-Active credit line reopening policy
    - Document field reset behavior table
    - Add backend integration considerations
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5_
  
  - [x] 11.4 Update events table
    - Clarify that "opened" event is emitted for both new and reopened credit lines
    - _Requirements: 6.5_
  
  - [x] 11.5 Review documentation for completeness
    - Verify all status behaviors are documented
    - Verify error messages match implementation
    - Verify field reset behavior is accurate
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5_

- [x] 12. Final checkpoint - Run full test suite and generate coverage report
  - Run cargo test to execute all tests
  - Run cargo llvm-cov --html to generate final coverage report
  - Verify 95% line coverage achieved
  - Verify all 30+ unit tests pass
  - Verify all 11 property tests pass (if implemented)
  - Ensure all tests pass, ask the user if questions arise.
  - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6, 7.7, 7.8_

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- The existing open_credit_line implementation already supports the duplicate open policy - no code changes needed
- Focus is on comprehensive testing and documentation to make implicit behavior explicit
- Property tests provide additional confidence but unit tests alone can achieve 95% coverage
- Each test references specific requirements for traceability
- Checkpoints ensure incremental validation
