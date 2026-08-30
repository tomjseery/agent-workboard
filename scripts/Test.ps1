$ErrorActionPreference = 'Stop'
$env:CARGO_BUILD_JOBS = '1'

cargo fmt --all --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo clippy --workspace --all-targets --all-features -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --workspace --all-targets
exit $LASTEXITCODE
