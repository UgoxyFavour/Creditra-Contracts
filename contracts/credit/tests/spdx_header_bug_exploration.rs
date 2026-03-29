// SPDX-License-Identifier: MIT

//! Bug Condition Exploration Test for Missing SPDX Headers
//!
//! **Validates: Requirements 1.1, 1.2, 1.3**
//!
//! This test verifies the bug condition exists BEFORE the fix is applied.
//! It checks that lib.rs and types.rs are missing SPDX headers while events.rs has one.
//!
//! **CRITICAL**: This test MUST FAIL on unfixed code - failure confirms the bug exists.
//! **EXPECTED OUTCOME**: Test FAILS (this is correct - it proves the bug exists)

use std::fs;
use std::path::Path;

/// **Property 1: Bug Condition** - Missing SPDX Headers in lib.rs and types.rs
///
/// This property-based test verifies that:
/// 1. lib.rs does NOT have the SPDX header (bug exists)
/// 2. types.rs does NOT have the SPDX header (bug exists)
/// 3. events.rs DOES have the SPDX header (baseline for comparison)
///
/// When this test FAILS, it confirms the bug condition exists.
/// When this test PASSES (after fix), it confirms the bug is resolved.
#[test]
fn property_bug_condition_missing_spdx_headers() {
    let expected_header = "// SPDX-License-Identifier: MIT";

    // Define the file paths relative to the workspace root
    let lib_path = Path::new("contracts/credit/src/lib.rs");
    let types_path = Path::new("contracts/credit/src/types.rs");
    let events_path = Path::new("contracts/credit/src/events.rs");

    // Read the first line of each file
    let lib_first_line = read_first_line(lib_path);
    let types_first_line = read_first_line(types_path);
    let events_first_line = read_first_line(events_path);

    // Baseline check: events.rs should have the correct header
    assert_eq!(
        events_first_line, expected_header,
        "events.rs should have SPDX header as baseline"
    );

    // Bug condition checks: lib.rs and types.rs should have the SPDX header
    // These assertions will FAIL on unfixed code (proving the bug exists)
    // and PASS on fixed code (proving the bug is resolved)
    assert_eq!(
        lib_first_line, expected_header,
        "lib.rs should have SPDX-License-Identifier header as first line"
    );

    assert_eq!(
        types_first_line, expected_header,
        "types.rs should have SPDX-License-Identifier header as first line"
    );
}

/// Helper function to read the first line of a file
fn read_first_line(path: &Path) -> String {
    // Try to find the file from the workspace root
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR")
        .map(|dir| {
            Path::new(&dir)
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf()
        })
        .unwrap_or_else(|_| std::env::current_dir().unwrap());

    let full_path = workspace_root.join(path);

    let content = fs::read_to_string(&full_path)
        .unwrap_or_else(|e| panic!("Failed to read file {:?}: {}", full_path, e));

    content.lines().next().unwrap_or("").to_string()
}

#[cfg(test)]
mod property_tests {
    use super::*;

    /// Property: All contract source files should have consistent SPDX headers
    ///
    /// This scoped property test focuses on the concrete failing cases:
    /// - lib.rs (currently missing header)
    /// - types.rs (currently missing header)
    /// - events.rs (baseline with correct header)
    #[test]
    fn property_all_contract_sources_have_spdx_headers() {
        let expected_header = "// SPDX-License-Identifier: MIT";

        // Test the specific files mentioned in the bug report
        let files_to_check = vec![
            "contracts/credit/src/lib.rs",
            "contracts/credit/src/types.rs",
            "contracts/credit/src/events.rs",
        ];

        for file_path in files_to_check {
            let path = Path::new(file_path);
            let first_line = read_first_line(path);

            assert_eq!(
                first_line, expected_header,
                "File {} should have SPDX-License-Identifier header as first line",
                file_path
            );
        }
    }

    /// Property: SPDX header format consistency
    ///
    /// Verifies that all files with SPDX headers use the exact same format:
    /// - Comment syntax: `//` (not `///` or `//!`)
    /// - Exact text: `SPDX-License-Identifier: MIT`
    /// - No extra whitespace or variations
    #[test]
    fn property_spdx_header_format_consistency() {
        let expected_header = "// SPDX-License-Identifier: MIT";

        let files_to_check = vec![
            "contracts/credit/src/lib.rs",
            "contracts/credit/src/types.rs",
            "contracts/credit/src/events.rs",
        ];

        for file_path in files_to_check {
            let path = Path::new(file_path);
            let first_line = read_first_line(path);

            // Check exact format match
            assert_eq!(
                first_line, expected_header,
                "File {} should have exact SPDX header format: '{}'",
                file_path, expected_header
            );

            // Additional format checks
            assert!(
                first_line.starts_with("// "),
                "File {} SPDX header should use '//' comment syntax",
                file_path
            );

            assert!(
                first_line.contains("SPDX-License-Identifier:"),
                "File {} should contain 'SPDX-License-Identifier:' in header",
                file_path
            );

            assert!(
                first_line.ends_with("MIT"),
                "File {} should specify MIT license",
                file_path
            );
        }
    }
}
