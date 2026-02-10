#!/bin/bash

echo "Validating Indexing Module Implementation..."
echo ""

# Check source files
echo "Checking source files..."
files=(
    "crates/ordne/src/index/mod.rs"
    "crates/ordne/src/index/device.rs"
    "crates/ordne/src/index/rmlint.rs"
    "crates/ordne/src/index/scanner.rs"
    "crates/ordne/src/index/hasher.rs"
    "crates/ordne/src/db/files.rs"
    "crates/ordne/src/db/duplicates.rs"
    "crates/ordne/src/db/drives.rs"
)

for file in "${files[@]}"; do
    if [ -f "$file" ]; then
        lines=$(wc -l < "$file")
        echo "  ✓ $file ($lines lines)"
    else
        echo "  ✗ $file (MISSING)"
    fi
done

echo ""
echo "Checking test files..."
test_files=(
    "tests/integration/mod.rs"
    "tests/integration/indexing_test.rs"
    "tests/fixtures/rmlint_sample.json"
)

for file in "${test_files[@]}"; do
    if [ -f "$file" ]; then
        lines=$(wc -l < "$file")
        echo "  ✓ $file ($lines lines)"
    else
        echo "  ✗ $file (MISSING)"
    fi
done

echo ""
echo "Checking documentation..."
if [ -f "docs/INDEXING_MODULE.md" ]; then
    lines=$(wc -l < "docs/INDEXING_MODULE.md")
    echo "  ✓ docs/INDEXING_MODULE.md ($lines lines)"
else
    echo "  ✗ docs/INDEXING_MODULE.md (MISSING)"
fi

echo ""
echo "Module summary:"
echo "  - Device discovery: discover_device(), discover_rclone_remote()"
echo "  - rmlint integration: RmlintParser, parse_rmlint_output()"
echo "  - Filesystem scanning: scan_directory(), ScanOptions"
echo "  - Hashing: hash_file_md5(), hash_file_blake3(), verify_hash()"
echo "  - DB operations: files, duplicates, drives modules"
echo ""
echo "To build and test (requires Rust installed):"
echo "  cargo build --release"
echo "  cargo test"
echo "  cargo test --test indexing_test"

