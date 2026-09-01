[CmdletBinding()]
param(
    [string]$InstallRoot = $PSScriptRoot,
    [switch]$RemoveData
)

$ErrorActionPreference = 'Stop'

if (-not [System.IO.Path]::IsPathRooted($InstallRoot)) {
    throw 'InstallRoot must be an absolute path.'
}

$InstallRoot = [System.IO.Path]::GetFullPath($InstallRoot)
$manifestPath = Join-Path $InstallRoot 'installed-manifest.json'
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Agent Workboard install manifest not found at $manifestPath"
}
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if ($manifest.owner -ne 'agent-workboard/windows-install-v1') {
    throw 'The install manifest is not owned by Agent Workboard.'
}
if ([System.IO.Path]::GetFullPath($manifest.installRoot) -ine $InstallRoot) {
    throw 'The install manifest belongs to another installation root.'
}

if ($manifest.pathAdded -eq $true) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @($userPath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    $entries = @($entries | Where-Object {
        [System.IO.Path]::GetFullPath($_).TrimEnd('\') -ine $InstallRoot.TrimEnd('\')
    })
    [Environment]::SetEnvironmentVariable('Path', ($entries -join ';'), 'User')
}

foreach ($relative in $manifest.files) {
    $candidate = [System.IO.Path]::GetFullPath((Join-Path $InstallRoot $relative))
    if (-not $candidate.StartsWith($InstallRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Install manifest path escapes the installation root: $relative"
    }
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
        Remove-Item -LiteralPath $candidate -Force
    }
}

Get-ChildItem -LiteralPath $InstallRoot -Directory -Recurse -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending |
    Where-Object { -not (Get-ChildItem -LiteralPath $_.FullName -Force) } |
    Remove-Item -Force
if ((Test-Path -LiteralPath $InstallRoot -PathType Container) -and
    -not (Get-ChildItem -LiteralPath $InstallRoot -Force)) {
    Remove-Item -LiteralPath $InstallRoot -Force
}

if ($RemoveData) {
    $projectDirectory = Join-Path $env:LOCALAPPDATA 'Agent Workboard\Agent Workboard'
    $dataRoot = [System.IO.Path]::GetFullPath((Join-Path $projectDirectory 'data'))
    $expectedParent = [System.IO.Path]::GetFullPath($projectDirectory).TrimEnd('\') + '\'
    if (-not $dataRoot.StartsWith($expectedParent, [StringComparison]::OrdinalIgnoreCase) -or
        [System.IO.Path]::GetFileName($dataRoot) -ine 'data') {
        throw "Refusing to remove an unexpected data directory: $dataRoot"
    }
    if (Test-Path -LiteralPath $dataRoot -PathType Container) {
        Remove-Item -LiteralPath $dataRoot -Recurse -Force
        Write-Output "Removed Agent Workboard data at $dataRoot"
    }
}

Write-Output "Uninstalled Agent Workboard from $InstallRoot"
Write-Output 'Provider configuration, planning stores, repositories, and worktrees were not changed.'
