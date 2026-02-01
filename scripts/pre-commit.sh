#!/bin/bash
# Pre-commit validation script
# Run this before committing to ensure code quality

set -e

echo "Running pre-commit checks..."

echo "1. Formatting code with cargo fmt..."
cargo fmt --all

echo "2. Running clippy..."
cargo clippy --all-targets --all-features -- -D warnings

echo "3. Building project..."
cargo build

echo "4. Running tests..."
cargo test

echo ""
echo "✅ All checks passed! Ready to commit."
