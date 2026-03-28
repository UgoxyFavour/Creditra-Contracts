#!/usr/bin/env python3
"""
Standalone verification script for SPDX headers
Verifies that all three contract source files have the correct SPDX header
"""

import sys
from pathlib import Path

def check_file(file_path, expected_first_line="// SPDX-License-Identifier: MIT"):
    """Check if a file has the expected SPDX header as its first line"""
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            first_line = f.readline().strip()
            
        if first_line == expected_first_line:
            print(f"✓ {file_path}: SPDX header present")
            return True
        else:
            print(f"✗ {file_path}: SPDX header missing or incorrect")
            print(f"  Expected: {expected_first_line}")
            print(f"  Got: {first_line}")
            return False
    except Exception as e:
        print(f"✗ {file_path}: Error reading file: {e}")
        return False

def main():
    print("=== SPDX Header Verification ===\n")
    
    files_to_check = [
        "contracts/credit/src/lib.rs",
        "contracts/credit/src/types.rs",
        "contracts/credit/src/events.rs",
    ]
    
    all_passed = True
    for file_path in files_to_check:
        if not check_file(file_path):
            all_passed = False
    
    print("\n=== Summary ===")
    if all_passed:
        print("✓ All files have correct SPDX headers")
        print("✓ Bug fix verified successfully")
        return 0
    else:
        print("✗ Some files are missing SPDX headers")
        return 1

if __name__ == "__main__":
    sys.exit(main())
