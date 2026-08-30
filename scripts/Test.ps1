$ErrorActionPreference = 'Stop'
$env:CARGO_BUILD_JOBS = '1'

cargo fmt --all --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo clippy --workspace --all-targets --all-features -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --workspace --all-targets
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Push-Location (Join-Path $PSScriptRoot '..\apps\workboard-desktop')
try {
    npm run generate:check
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npm run typecheck
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npm test
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npm run build
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
