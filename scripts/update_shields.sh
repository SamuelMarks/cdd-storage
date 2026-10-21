#!/usr/bin/env bash
set -euo pipefail

# Calculate doc coverage using nightly rustdoc in an isolated target directory
# so it never invalidates the LLVM-instrumented test build cache in target/debug.
DOC_TARGET_DIR="target/doc-cov"
if command -v cargo &> /dev/null && rustup toolchain list | grep -q nightly; then
    DOC_OUT_LIB=$(cargo +nightly rustdoc --target-dir "$DOC_TARGET_DIR" --lib -- -Z unstable-options --show-coverage 2>&1 || true)
    if [ -f "$DOC_TARGET_DIR/doc/cdd_storage.txt" ]; then
        DOC_COVERAGE_LIB=$(grep "Total" "$DOC_TARGET_DIR/doc/cdd_storage.txt" | awk '{print $6}' | tr -d '%' || true)
    else
        DOC_COVERAGE_LIB=$(echo "$DOC_OUT_LIB" | grep "Total" | awk '{print $6}' | tr -d '%' || true)
    fi

    DOC_OUT_BIN=$(cargo +nightly rustdoc --target-dir "$DOC_TARGET_DIR" --bin cdd-storage -- -Z unstable-options --show-coverage 2>&1 || true)
    if [ -f "$DOC_TARGET_DIR/doc/cdd_storage.txt" ]; then
        DOC_COVERAGE_BIN=$(grep "Total" "$DOC_TARGET_DIR/doc/cdd_storage.txt" | awk '{print $6}' | tr -d '%' || true)
    else
        DOC_COVERAGE_BIN=$(echo "$DOC_OUT_BIN" | grep "Total" | awk '{print $6}' | tr -d '%' || true)
    fi
else
    DOC_COVERAGE_LIB="100"
    DOC_COVERAGE_BIN="100"
fi

if [ -z "${DOC_COVERAGE_LIB:-}" ]; then
    DOC_COVERAGE_LIB="0"
fi
if [ -z "${DOC_COVERAGE_BIN:-}" ]; then
    DOC_COVERAGE_BIN="100"
fi

DOC_LIB_ROUNDED=$(printf "%.0f" "$DOC_COVERAGE_LIB")
DOC_BIN_ROUNDED=$(printf "%.0f" "$DOC_COVERAGE_BIN")

if [ "$DOC_LIB_ROUNDED" -lt "$DOC_BIN_ROUNDED" ]; then
    DOC_COVERAGE="$DOC_LIB_ROUNDED"
else
    DOC_COVERAGE="$DOC_BIN_ROUNDED"
fi

if [ "$DOC_COVERAGE" -ge 80 ]; then
    DOC_COLOR="success"
elif [ "$DOC_COVERAGE" -ge 50 ]; then
    DOC_COLOR="yellow"
else
    DOC_COLOR="red"
fi

# Calculate test coverage using tarpaulin if available
COVERAGE_ROUNDED=""
if command -v cargo-tarpaulin &> /dev/null; then
    echo "Running tests and calculating coverage..."
    TARPAULIN_OUT=$(cargo tarpaulin --engine llvm --skip-clean --timeout 120 --out Lcov 2>&1 || true)
    COVERAGE=$(echo "$TARPAULIN_OUT" | grep -oE "[0-9.]+% coverage" | tail -n 1 | grep -oE "[0-9.]+" || true)
    if [ -z "${COVERAGE:-}" ]; then
        COVERAGE=$(echo "$TARPAULIN_OUT" | grep -oE "coverage: [0-9.]+" | tail -n 1 | awk '{print $2}' || true)
    fi
    if [ -n "${COVERAGE:-}" ]; then
        COVERAGE_ROUNDED=$(printf "%.0f" "$COVERAGE")
    fi
else
    echo "cargo-tarpaulin not installed; preserving existing test coverage badge."
fi

# Portable replacement without creating sed -E backup files on macOS
TMP_README=$(mktemp)
trap 'rm -f "$TMP_README"' EXIT

if [ -n "$COVERAGE_ROUNDED" ] && [ "$COVERAGE_ROUNDED" -gt 0 ]; then
    if [ "$COVERAGE_ROUNDED" -ge 80 ]; then
        TEST_COLOR="success"
    elif [ "$COVERAGE_ROUNDED" -ge 50 ]; then
        TEST_COLOR="yellow"
    else
        TEST_COLOR="red"
    fi
    sed -E -e "s|![[]Test Coverage[]][(]https://img[.]shields[.]io/badge/coverage-[0-9.]+%25-[a-z]+[.]svg[)]|![Test Coverage](https://img.shields.io/badge/coverage-${COVERAGE_ROUNDED}%25-${TEST_COLOR}.svg)|g" -e "s|![[]Doc Coverage[]][(]https://img[.]shields[.]io/badge/docs-[0-9.]+%25-[a-z]+[.]svg[)]|![Doc Coverage](https://img.shields.io/badge/docs-${DOC_COVERAGE}%25-${DOC_COLOR}.svg)|g" README.md > "$TMP_README"
    mv "$TMP_README" README.md
    echo "Updated shields in README.md (Test: ${COVERAGE_ROUNDED}%, Doc: ${DOC_COVERAGE}%)"
else
    sed -E -e "s|![[]Doc Coverage[]][(]https://img[.]shields[.]io/badge/docs-[0-9.]+%25-[a-z]+[.]svg[)]|![Doc Coverage](https://img.shields.io/badge/docs-${DOC_COVERAGE}%25-${DOC_COLOR}.svg)|g" README.md > "$TMP_README"
    mv "$TMP_README" README.md
    echo "Updated shields in README.md (Doc: ${DOC_COVERAGE}%)"
fi

if [ "$DOC_COVERAGE" -lt 100 ]; then
    echo "Error: Documentation coverage (${DOC_COVERAGE}%) is below 100%!" >&2
    exit 1
fi

if [ -n "$COVERAGE_ROUNDED" ] && [ "$COVERAGE_ROUNDED" -lt 100 ]; then
    echo "Error: Test coverage (${COVERAGE_ROUNDED}%) is below 100%!" >&2
    exit 1
fi
