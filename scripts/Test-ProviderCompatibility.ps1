[CmdletBinding()]
param(
    [string]$Workboard = 'workboard',
    [string]$Claude = 'claude',
    [string]$Codex = 'codex',
    [string]$Database = (Join-Path $env:TEMP ('agent-workboard-provider-check-' + [guid]::NewGuid().ToString('N') + '.sqlite'))
)

$ErrorActionPreference = 'Stop'
$claudeHome = if ($env:CLAUDE_CONFIG_DIR) { $env:CLAUDE_CONFIG_DIR } else { Join-Path $env:USERPROFILE '.claude' }
$codexHome = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $env:USERPROFILE '.codex' }

function Resolve-ProviderCommand([string]$Name, [string]$Command) {
    $resolved = Get-Command $Command -ErrorAction SilentlyContinue
    if ($resolved) {
        return $resolved.Source
    }
    if ($Name -eq 'Codex') {
        $bin = Join-Path $env:LOCALAPPDATA 'OpenAI\Codex\bin'
        if (Test-Path -LiteralPath $bin -PathType Container) {
            $appCli = Get-ChildItem -LiteralPath $bin -Filter codex.exe -File -Recurse |
                Sort-Object LastWriteTime -Descending |
                Select-Object -First 1
            if ($appCli) {
                return $appCli.FullName
            }
        }
    }
    throw "$Name executable was not found: $Command"
}

foreach ($provider in @(
    [pscustomobject]@{ Name = 'Claude'; Command = $Claude; Tool = 'claude'; Home = $claudeHome },
    [pscustomobject]@{ Name = 'Codex'; Command = $Codex; Tool = 'codex'; Home = $codexHome }
)) {
    $resolved = Resolve-ProviderCommand $provider.Name $provider.Command
    $version = & $resolved --version
    if ($LASTEXITCODE -ne 0) {
        throw "$($provider.Name) did not report a supported version."
    }
    Write-Output "$($provider.Name): $($version -join ' ')"
    & $Workboard --database $Database --json integration status --tool $provider.Tool --home $provider.Home | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "$($provider.Name) integration compatibility check failed."
    }
}

& $Workboard --database $Database --json diagnostics --claude-home $claudeHome --codex-home $codexHome | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw 'Workboard diagnostics failed after provider compatibility checks.'
}
Write-Output 'Claude and Codex compatibility checks passed.'
