$ErrorActionPreference = 'Stop'
$env:CARGO_BUILD_JOBS = '1'

cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
