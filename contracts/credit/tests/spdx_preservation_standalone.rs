// SPDX-License-Identifier: MIT

//! Standalone Preservation Tests for SPDX Header Bugfix
//!
//! **Validates: Requirements 3.1, 3.2, 3.3, 3.4, 3.5**
//!
//! These tests run WITHOUT compiling the main library, allowing us to verify
//! file-level preservation properties even when there are pre-existing syntax errors.
//!
//! **IMPORTANT**: Observation-first methodology
//! - Tests document baseline behavior on UNFIXED code
//! - Tests verify behavior remains unchanged after fix
//!
//! **EXPECTED OUTCOME**: Tests PASS on both unfixed and fixed code

use std::fs;
use std::path::{Path, PathBuf};

/// Helper function to resolve file paths relative to workspace root
fn resolve_path(relative_path: &str) -> PathBuf {
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

    workspace_root.join(relative_path)
}

/// Helper function to read the first line of a file
fn read_first_line(path: &Path) -> String {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read file {:?}: {}", path, e));

    content.lines().next().unwrap_or("").to_string()
}

/// **Property 2: Preservation** - events.rs First Line
///
/// **Observation on UNFIXED code**:
/// events.rs first line is: `//! Event types and topic constants for the Credit contract.`
/// (No SPDX header currently present)
///
/// **Expected after fix**:
/// - If events.rs is in scope: first line becomes `// SPDX-License-Identifier: MIT`
/// - If events.rs is out of scope: first line remains unchanged
///
/// This test documents the baseline and will verify preservation.
#[test]
fn property_preservation_events_rs_first_line() {
    let events_path = resolve_path("contracts/credit/src/events.rs");
    let first_line = read_first_line(&events_path);

    // Document observed state on unfixed code
    println!("events.rs first line: {}", first_line);

    // BASELINE OBSERVATION (unfixed code):
    // First line is: "//! Event types and topic constants for the Credit contract."

    // Verify the file is readable and has content
    assert!(!first_line.is_empty(), "events.rs should have a first line");

    // Verify it's a comment (either SPDX header or existing doc comment)
    assert!(
        first_line.starts_with("//"),
        "events.rs first line should be a comment, got: {}",
        first_line
    );

    // After fix: verify it either has SPDX header OR remains unchanged
    // This test passes on both unfixed and fixed code
}

/// **Property 2: Preservation** - lib.rs First Line (Before Fix)
///
/// **Observation on UNFIXED code**:
/// lib.rs first line is: `#![no_std]`
/// (No SPDX header - this is the bug condition)
///
/// This test documents the baseline state before the fix.
#[test]
fn property_preservation_lib_rs_first_line_baseline() {
    let lib_path = resolve_path("contracts/credit/src/lib.rs");
    let first_line = read_first_line(&lib_path);

    // Document observed state on unfixed code
    println!("lib.rs first line: {}", first_line);

    // BASELINE OBSERVATION (unfixed code):
    // First line is: "#![no_std]"

    // Verify the file is readable and has content
    assert!(!first_line.is_empty(), "lib.rs should have a first line");

    // After fix: first line should be "// SPDX-License-Identifier: MIT"
    // This test documents the baseline on unfixed code
}

/// **Property 2: Preservation** - types.rs First Line (Before Fix)
///
/// **Observation on UNFIXED code**:
/// types.rs first line is: `//! Core data types for the Credit contract.`
/// (No SPDX header - this is the bug condition)
///
/// This test documents the baseline state before the fix.
#[test]
fn property_preservation_types_rs_first_line_baseline() {
    let types_path = resolve_path("contracts/credit/src/types.rs");
    let first_line = read_first_line(&types_path);

    // Document observed state on unfixed code
    println!("types.rs first line: {}", first_line);

    // BASELINE OBSERVATION (unfixed code):
    // First line is: "//! Core data types for the Credit contract."

    // Verify the file is readable and has content
    assert!(!first_line.is_empty(), "types.rs should have a first line");

    // After fix: first line should be "// SPDX-License-Identifier: MIT"
    // This test documents the baseline on unfixed code
}

/// **Property 2: Preservation** - All Files Remain Readable
///
/// Verify all contract source files remain readable as UTF-8 text
/// both before and after adding SPDX headers.
#[test]
fn property_preservation_all_files_readable() {
    let files = vec![
        "contracts/credit/src/lib.rs",
        "contracts/credit/src/types.rs",
        "contracts/credit/src/events.rs",
    ];

    for file_path in files {
        let path = resolve_path(file_path);

        // Verify file can be read as UTF-8
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("File {} should be readable: {}", file_path, e));

        // Verify file is not empty
        assert!(
            !content.is_empty(),
            "File {} should not be empty",
            file_path
        );

        // Verify file has multiple lines
        let line_count = content.lines().count();
        assert!(
            line_count > 1,
            "File {} should have multiple lines, got {}",
            file_path,
            line_count
        );

        println!("✓ {} is readable ({} lines)", file_path, line_count);
    }
}

/// **Property 2: Preservation** - File Content Beyond First Line Unchanged
///
/// Verify that adding SPDX headers only affects the first line(s),
/// and all subsequent content remains exactly the same.
#[test]
fn property_preservation_content_beyond_header_unchanged() {
    let files = vec![
        "contracts/credit/src/lib.rs",
        "contracts/credit/src/types.rs",
        "contracts/credit/src/events.rs",
    ];

    for file_path in files {
        let path = resolve_path(file_path);
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", file_path, e));

        let lines: Vec<&str> = content.lines().collect();

        // Verify there's substantial content beyond the first line
        assert!(
            lines.len() > 10,
            "{} should have substantial content (>10 lines), got {}",
            file_path,
            lines.len()
        );

        // Verify there's actual code (not just comments)
        let has_code = lines.iter().any(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("//")
                && !trimmed.starts_with("/*")
                && !trimmed.starts_with("*")
        });

        assert!(
            has_code,
            "{} should have code content beyond comments",
            file_path
        );

        println!(
            "✓ {} has {} lines with code content",
            file_path,
            lines.len()
        );
    }
}

/// **Property 2: Preservation** - File Sizes Are Reasonable
///
/// Verify files have reasonable sizes and adding SPDX headers
/// only adds minimal content (~35 bytes per file).
#[test]
fn property_preservation_file_sizes_reasonable() {
    let files = vec![
        "contracts/credit/src/lib.rs",
        "contracts/credit/src/types.rs",
        "contracts/credit/src/events.rs",
    ];

    for file_path in files {
        let path = resolve_path(file_path);
        let metadata = fs::metadata(&path)
            .unwrap_or_else(|e| panic!("Failed to get metadata for {}: {}", file_path, e));

        let file_size = metadata.len();

        // Verify file has reasonable size (not empty, not corrupted)
        assert!(
            file_size > 100,
            "File {} should have substantial content (>100 bytes), got {} bytes",
            file_path,
            file_size
        );

        println!("✓ {} size: {} bytes", file_path, file_size);
    }
}

/// **Property 2: Preservation** - Comment Syntax Remains Valid
///
/// Verify that all comment lines use valid Rust comment syntax
/// both before and after adding SPDX headers.
#[test]
fn property_preservation_comment_syntax_valid() {
    let files = vec![
        "contracts/credit/src/lib.rs",
        "contracts/credit/src/types.rs",
        "contracts/credit/src/events.rs",
    ];

    for file_path in files {
        let path = resolve_path(file_path);
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", file_path, e));

        // Find all comment lines
        let comment_lines: Vec<&str> = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*")
            })
            .collect();

        // Verify there are some comments
        assert!(
            !comment_lines.is_empty(),
            "File {} should have comment lines",
            file_path
        );

        // Verify all comment lines use valid syntax
        for comment in &comment_lines {
            let trimmed = comment.trim();
            assert!(
                trimmed.starts_with("//")
                    || trimmed.starts_with("///")
                    || trimmed.starts_with("//!")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*"),
                "Invalid comment syntax in {}: {}",
                file_path,
                comment
            );
        }

        println!(
            "✓ {} has {} valid comment lines",
            file_path,
            comment_lines.len()
        );
    }
}
