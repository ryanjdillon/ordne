#!/usr/bin/env bash
set -euo pipefail

echo "=== Prune Phase 1 Foundation Verification ==="
echo

echo "1. Checking project structure..."
if [[ -f "Cargo.toml" ]] && [[ -f "flake.nix" ]] && [[ -f "LICENSE" ]] && [[ -f "README.md" ]]; then
    echo "   ✓ Root files present"
else
    echo "   ✗ Missing root files"
    exit 1
fi

if [[ -f "crates/prune/Cargo.toml" ]] && [[ -f "crates/prune/src/lib.rs" ]] && [[ -f "crates/prune/src/main.rs" ]]; then
    echo "   ✓ Prune crate structure correct"
else
    echo "   ✗ Prune crate structure incomplete"
    exit 1
fi

echo
echo "2. Checking core modules..."
required_files=(
    "crates/prune/src/error.rs"
    "crates/prune/src/config.rs"
    "crates/prune/src/db/mod.rs"
    "crates/prune/src/db/schema.rs"
    "tests/common/mod.rs"
)

for file in "${required_files[@]}"; do
    if [[ -f "$file" ]]; then
        echo "   ✓ $file"
    else
        echo "   ✗ Missing: $file"
        exit 1
    fi
done

echo
echo "3. Verifying schema contains all tables..."
required_tables=(
    "drives"
    "files"
    "duplicate_groups"
    "migration_plans"
    "migration_steps"
    "audit_log"
    "schema_version"
)

for table in "${required_tables[@]}"; do
    if grep -q "CREATE TABLE.*${table}" crates/prune/src/db/schema.rs; then
        echo "   ✓ Table: $table"
    else
        echo "   ✗ Missing table: $table"
        exit 1
    fi
done

echo
echo "4. Checking for indexes..."
if grep -q "CREATE INDEX" crates/prune/src/db/schema.rs; then
    index_count=$(grep -c "CREATE INDEX" crates/prune/src/db/schema.rs || true)
    echo "   ✓ Found $index_count indexes"
else
    echo "   ✗ No indexes found"
    exit 1
fi

echo
echo "5. Attempting to build (requires Rust toolchain)..."
if command -v cargo &> /dev/null; then
    echo "   Building..."
    if cargo build 2>&1 | tee build.log; then
        echo "   ✓ Build successful"
    else
        echo "   ✗ Build failed - see build.log"
        exit 1
    fi
else
    echo "   ⚠ Cargo not available - skipping build"
    echo "   Run 'nix develop' to enter development shell"
fi

echo
echo "6. Running tests (if cargo available)..."
if command -v cargo &> /dev/null; then
    if cargo test 2>&1 | tee test.log; then
        echo "   ✓ Tests passed"
    else
        echo "   ✗ Tests failed - see test.log"
        exit 1
    fi
else
    echo "   ⚠ Cargo not available - skipping tests"
fi

echo
echo "=== Phase 1 Foundation: VERIFIED ==="
echo
echo "Next steps:"
echo "  1. Enter dev shell: nix develop"
echo "  2. Build project: cargo build"
echo "  3. Run tests: cargo test"
echo "  4. Check CLI: cargo run -- --help"
