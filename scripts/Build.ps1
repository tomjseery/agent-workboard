$ErrorActionPreference = 'Stop'
$env:CARGO_BUILD_JOBS = '1'

cargo build --workspace --all-targets
