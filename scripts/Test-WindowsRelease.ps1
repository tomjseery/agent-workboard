[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)]
    [string]$Archive
)

$ErrorActionPreference = 'Stop'
$Archive = [System.IO.Path]::GetFullPath($Archive)
if (-not (Test-Path -LiteralPath $Archive -PathType Leaf)) {
    throw "Release archive not found: $Archive"
}

$testRoot = Join-Path $env:TEMP ('agent-workboard-release-test-' + [guid]::NewGuid().ToString('N'))
$extractRoot = Join-Path $testRoot 'release'
$installRoot = Join-Path $testRoot 'install'
$runtimeRoot = Join-Path $testRoot 'runtime'
$claudeHome = Join-Path $testRoot 'providers\claude'
$codexHome = Join-Path $testRoot 'providers\codex'
$database = Join-Path $testRoot 'data\workboard.sqlite'
$planningStore = Join-Path $testRoot 'planning-store'
$repository = Join-Path $testRoot 'repository'

function Get-TreeDigest([string]$Root) {
    $lines = @(Get-ChildItem -LiteralPath $Root -File -Recurse | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($Root.Length).TrimStart('\').Replace('\', '/')
        "$relative=$((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash)"
    })
    $bytes = [Text.Encoding]::UTF8.GetBytes($lines -join "`n")
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '')
    }
    finally {
        $sha.Dispose()
    }
}

New-Item -ItemType Directory -Force -Path $extractRoot, $runtimeRoot, $claudeHome, $codexHome, $repository | Out-Null
Set-Content -LiteralPath (Join-Path $claudeHome 'foreign-settings.json') -Value '{"owner":"provider"}' -Encoding utf8
Set-Content -LiteralPath (Join-Path $codexHome 'foreign-config.toml') -Value 'owner = "provider"' -Encoding utf8
$providerDigest = "$(Get-TreeDigest $claudeHome):$(Get-TreeDigest $codexHome)"

try {
    Expand-Archive -LiteralPath $Archive -DestinationPath $extractRoot
    & (Join-Path $extractRoot 'Install.ps1') -InstallRoot $installRoot -NoPath

    $workboard = Join-Path $installRoot 'workboard.exe'
    $daemon = Join-Path $installRoot 'workboard-daemon.exe'
    if (-not (Test-Path -LiteralPath $workboard -PathType Leaf) -or
        -not (Test-Path -LiteralPath $daemon -PathType Leaf)) {
        throw 'The installed CLI or daemon is missing.'
    }

    $git = Get-Command git -CommandType Application -ErrorAction Stop
    $systemPath = [Environment]::GetFolderPath('System')
    $originalPath = $env:PATH
    $env:PATH = "$installRoot;$($git.Source | Split-Path -Parent);$systemPath"
    Push-Location $runtimeRoot
    try {
        & $workboard --version | Out-Null
        & $daemon --version | Out-Null
        & $workboard --database $database --json diagnostics --claude-home $claudeHome --codex-home $codexHome | Out-Null
        & $workboard --database $database --json init --store $planningStore --slug release-test --title 'Release test' | Out-Null

        & $git.Source -C $repository init | Out-Null
        Set-Content -LiteralPath (Join-Path $repository 'README.md') -Value '# Release fixture' -Encoding utf8
        & $git.Source -C $repository add README.md
        & $git.Source -C $repository -c user.name='Agent Workboard Test' -c user.email='workboard@example.invalid' commit -m 'Initial fixture' | Out-Null
        & $workboard --database $database --json repository add $repository --slug release-fixture --title 'Release fixture' | Out-Null
        & $workboard --database $database --json epic create release --slug release | Out-Null
        & $workboard --database $database --json show | Out-Null
        & $workboard --database $database --json recover --dry-run | Out-Null
        & $workboard --database $database backup (Join-Path $testRoot 'backup\workboard.sqlite') | Out-Null
        & $workboard --database $database export (Join-Path $testRoot 'export') | Out-Null
        foreach ($command in 'plan', 'feature', 'work', 'session', 'recover') {
            $help = & $workboard $command --help
            if ($LASTEXITCODE -ne 0 -or -not $help) {
                throw "Packaged command is unavailable: $command"
            }
        }

        $daemonProcess = Start-Process $daemon -ArgumentList @(
            '--database', $database,
            '--claude-root', $claudeHome,
            '--codex-root', $codexHome
        ) -WindowStyle Hidden -PassThru
        try {
            $endpoint = [System.IO.Path]::ChangeExtension($database, 'workboardd.json')
            $deadline = [DateTime]::UtcNow.AddSeconds(10)
            while (-not (Test-Path -LiteralPath $endpoint) -and [DateTime]::UtcNow -lt $deadline) {
                Start-Sleep -Milliseconds 100
            }
            if (-not (Test-Path -LiteralPath $endpoint)) {
                throw 'The packaged daemon did not publish its loopback endpoint.'
            }
            $diagnostics = & $workboard --database $database --json diagnostics --claude-home $claudeHome --codex-home $codexHome | ConvertFrom-Json
            if ($diagnostics.daemonAvailable -ne $true) {
                throw 'Diagnostics did not observe the packaged daemon.'
            }
        }
        finally {
            Stop-Process -Id $daemonProcess.Id -Force -ErrorAction SilentlyContinue
            $daemonProcess.WaitForExit()
        }
    }
    finally {
        Pop-Location
        $env:PATH = $originalPath
    }

    & (Join-Path $installRoot 'Uninstall.ps1') -InstallRoot $installRoot
    if ((Test-Path -LiteralPath $workboard -PathType Leaf) -or
        (Test-Path -LiteralPath $daemon -PathType Leaf)) {
        throw 'Uninstall left a release executable behind.'
    }
    $afterDigest = "$(Get-TreeDigest $claudeHome):$(Get-TreeDigest $codexHome)"
    if ($afterDigest -ne $providerDigest) {
        throw 'Installation, diagnostics, daemon use, or uninstall changed provider configuration.'
    }
    if (-not (Test-Path -LiteralPath $database -PathType Leaf)) {
        throw 'Uninstall removed the Workboard database without an explicit data-removal request.'
    }
    Write-Output 'Windows release installation, source-free workflow smoke test, daemon, isolation, and uninstall passed.'
}
finally {
    if (Test-Path -LiteralPath $testRoot) {
        $resolvedTemp = [System.IO.Path]::GetFullPath($env:TEMP).TrimEnd('\') + '\'
        $resolvedTest = [System.IO.Path]::GetFullPath($testRoot)
        if ($resolvedTest.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase) -and
            [System.IO.Path]::GetFileName($resolvedTest).StartsWith('agent-workboard-release-test-')) {
            Remove-Item -LiteralPath $resolvedTest -Recurse -Force
        }
    }
}
