#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Database,
    [switch]$Rebuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$log = Join-Path $root 'desktop-run.log'
$desktop = Join-Path $root 'target\release\workboard-desktop.exe'
$cli = Join-Path $root 'target\release\workboard.exe'

function Write-Both([string]$text) {
    Write-Host $text
    Add-Content -LiteralPath $log -Value $text -Encoding utf8
}

Set-Content -LiteralPath $log -Value "Agent Workboard desktop run $(Get-Date -Format o)" -Encoding utf8

if ($Rebuild -or -not (Test-Path $desktop) -or -not (Test-Path $cli)) {
    Write-Both 'Building workboard.exe...'
    Push-Location $root
    try {
        cargo build --release -p workboard-cli
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    }
    finally { Pop-Location }

    Write-Both 'Building the desktop application...'
    Push-Location (Join-Path $root 'apps\workboard-desktop')
    try {
        npx tauri build --no-bundle
        if ($LASTEXITCODE -ne 0) { throw "tauri build failed with exit code $LASTEXITCODE" }
    }
    finally { Pop-Location }
}

if (-not $Database) {
    $Database = Join-Path $env:LOCALAPPDATA 'Agent Workboard\Agent Workboard\data\workboard.sqlite'
}
Write-Both "Database: $Database"

if (-not (Test-Path $Database)) {
    Write-Both 'No database at that path. Creating one.'
    $store = Join-Path (Split-Path -Parent $Database) 'store'
    & $cli --database $Database init --store $store 2>&1 | ForEach-Object { Write-Both $_ }
    if ($LASTEXITCODE -ne 0) { Write-Both 'Could not initialise a workspace.'; Read-Host 'Press Enter to close'; exit 1 }
}

Write-Both 'Checking the daemon can open this database...'
$probeOut = Join-Path $env:TEMP 'workboard-daemon-probe.log'
$probe = Start-Process -FilePath $cli -ArgumentList @('--database', $Database, 'daemon') `
    -RedirectStandardError $probeOut -NoNewWindow -PassThru
Start-Sleep -Seconds 3
if ($probe.HasExited) {
    $reason = (Get-Content -LiteralPath $probeOut -Raw -ErrorAction SilentlyContinue).Trim()
    Write-Both "The daemon could not open this database: $reason"
    Write-Both 'The desktop application cannot show anything until this is resolved.'
    Read-Host 'Press Enter to close'
    exit 1
}
Stop-Process -Id $probe.Id -Force -ErrorAction SilentlyContinue
Write-Both 'The daemon opened this database successfully.'

$env:WORKBOARD_DATABASE = $Database
Write-Both 'Starting the desktop application...'
& $desktop 2>&1 | ForEach-Object { Write-Both $_ }
$code = $LASTEXITCODE

Write-Both "The desktop application exited with code $code."
Write-Both "Full log: $log"
if ($code -ne 0) { Read-Host 'It exited with an error. Press Enter to close' }
