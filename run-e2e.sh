#!/bin/bash
set -e

echo "=== Rustus End-to-End: Rust → SIR JSON → Scalus → UPLC ==="
echo

# Step 1: Build and run the Rust example
echo "--- Step 1: Building Rust and generating SIR JSON ---"
cargo run --example validator
echo

# Step 2: Run the Scala loader
echo "--- Step 2: Loading SIR in Scalus, lowering to UPLC ---"
cd scala-loader
sbt --error "run ../my_validator.sir.json"
cd ..

# Cleanup
rm -f my_validator.sir.json
