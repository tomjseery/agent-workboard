[CmdletBinding()]
param(
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA 'Programs\Agent Workboard'),
    [switch]$NoPath
)

$ErrorActionPreference = 'Stop'

if (-not [System.IO.Path]::IsPathRooted($InstallRoot)) {
    throw 'InstallRoot must be an absolute path.'
}

$InstallRoot = [System.IO.Path]::GetFullPath($InstallRoot)
$releaseManifestPath = Join-Path $PSScriptRoot 'release-manifest.json'
$releaseManifest = Get-Content -Raw -LiteralPath $releaseManifestPath | ConvertFrom-Json

foreach ($file in $releaseManifest.files) {
    $source = Join-Path $PSScriptRoot $file.path
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "Release file is missing: $($file.path)"
    }
    $actual = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $file.sha256) {
        throw "Release file failed SHA-256 verification: $($file.path)"
    }
}

New-Item -ItemType Directory -Force -Path $InstallRoot | Out-Null
$previousManifestPath = Join-Path $InstallRoot 'installed-manifest.json'
$previousPathAdded = $false
$previousFiles = @()
if (Test-Path -LiteralPath $previousManifestPath -PathType Leaf) {
    $previous = Get-Content -Raw -LiteralPath $previousManifestPath | ConvertFrom-Json
    if ($previous.owner -ne 'agent-workboard/windows-install-v1' -or
        [System.IO.Path]::GetFullPath($previous.installRoot) -ine $InstallRoot) {
        throw 'The existing install manifest is not owned by this Agent Workboard installation.'
    }
    $previousPathAdded = $previous.pathAdded -eq $true
    $previousFiles = @($previous.files)
}

foreach ($file in $releaseManifest.files) {
    $source = Join-Path $PSScriptRoot $file.path
    $destination = Join-Path $InstallRoot $file.path
    $destinationDirectory = Split-Path -Parent $destination
    New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null
    $staged = "$destination.installing"
    Copy-Item -LiteralPath $source -Destination $staged -Force
    Move-Item -LiteralPath $staged -Destination $destination -Force
}
foreach ($controlFile in 'release-manifest.json', 'SHA256SUMS') {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $controlFile) -Destination (Join-Path $InstallRoot $controlFile) -Force
}

$currentFiles = @($releaseManifest.files.path) + 'release-manifest.json' + 'SHA256SUMS' + 'installed-manifest.json'
foreach ($relative in $previousFiles | Where-Object { $_ -notin $currentFiles }) {
    $obsolete = [System.IO.Path]::GetFullPath((Join-Path $InstallRoot $relative))
    if (-not $obsolete.StartsWith($InstallRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Existing install manifest path escapes the installation root: $relative"
    }
    if (Test-Path -LiteralPath $obsolete -PathType Leaf) {
        Remove-Item -LiteralPath $obsolete -Force
    }
}

$pathAdded = $previousPathAdded
if (-not $NoPath) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @($userPath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    $present = $entries | Where-Object {
        [System.IO.Path]::GetFullPath($_).TrimEnd('\') -ieq $InstallRoot.TrimEnd('\')
    }
    if (-not $present) {
        $entries += $InstallRoot
        [Environment]::SetEnvironmentVariable('Path', ($entries -join ';'), 'User')
        $pathAdded = $true
    }
}

$installedManifest = [ordered]@{
    owner = 'agent-workboard/windows-install-v1'
    version = $releaseManifest.version
    installRoot = $InstallRoot
    pathAdded = $pathAdded
    files = $currentFiles
}
$installedManifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $previousManifestPath -Encoding utf8

Write-Output "Installed Agent Workboard $($releaseManifest.version) to $InstallRoot"
if (-not $NoPath) {
    Write-Output 'Open a new terminal before invoking workboard from PATH.'
}
