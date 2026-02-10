#!/bin/bash
# Run integration tests with proper g3 binary path
echo "Building release binary..."
cargo build --release
export CARGO_BIN_EXE_g3="$(pwd)/target/release/g3"
cargo test -p g3-cli --test cli_integration_test "$@"
