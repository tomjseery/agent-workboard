[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$CurrentVersion,
    [string]$InstallRoot = $PSScriptRoot,
    [int]$ParentProcessId = 0,
    [switch]$Check,
    [string]$Version,
    [string]$Repository = 'tomjseery/agent-workboard',
    [string]$Archive,
    [string]$Checksum,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$target = 'x86_64-pc-windows-msvc'
$InstallRoot = [System.IO.Path]::GetFullPath($InstallRoot)

function Convert-ReleaseVersion([string]$Value) {
    $normalized = $Value.Trim().TrimStart('v')
    $core = ($normalized -split '-', 2)[0]
    try {
        [Version]$core
    }
    catch {
        throw "Release version is invalid: $Value"
    }
}

function Get-OwnedManifest([string]$Root) {
    $manifestPath = Join-Path $Root 'release-manifest.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw 'The update archive has no release manifest.'
    }
    $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
    if ($manifest.owner -ne 'agent-workboard/windows-release-v1' -or $manifest.target -ne $target) {
        throw 'The update archive is not an Agent Workboard Windows release.'
    }
    foreach ($file in $manifest.files) {
        $path = [System.IO.Path]::GetFullPath((Join-Path $Root $file.path))
        if (-not $path.StartsWith($Root.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
            throw "Release manifest path escapes the archive: $($file.path)"
        }
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Release file is missing: $($file.path)"
        }
        $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $file.sha256) {
            throw "Release file failed SHA-256 verification: $($file.path)"
        }
    }
    $manifest
}

$temporaryRoot = Join-Path $env:TEMP ('agent-workboard-update-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporaryRoot | Out-Null
try {
    $releaseVersion = $null
    $archivePath = $null
    $checksumPath = $null

    if ($Archive) {
        if (-not $Checksum) {
            throw 'Checksum is required with Archive.'
        }
        $archivePath = [System.IO.Path]::GetFullPath($Archive)
        $checksumPath = [System.IO.Path]::GetFullPath($Checksum)
    }
    else {
        $endpoint = if ($Version) {
            "https://api.github.com/repos/$Repository/releases/tags/v$($Version.TrimStart('v'))"
        }
        else {
            "https://api.github.com/repos/$Repository/releases/latest"
        }
        $headers = @{ 'User-Agent' = 'agent-workboard-updater' }
        $release = Invoke-RestMethod -Uri $endpoint -Headers $headers
        $releaseVersion = $release.tag_name.TrimStart('v')
        $wantedArchive = "agent-workboard-$releaseVersion-$target.zip"
        $wantedChecksum = "$wantedArchive.sha256"
        $archiveAsset = @($release.assets | Where-Object { $_.name -eq $wantedArchive })
        $checksumAsset = @($release.assets | Where-Object { $_.name -eq $wantedChecksum })
        if ($archiveAsset.Count -ne 1 -or $checksumAsset.Count -ne 1) {
            throw "Release v$releaseVersion does not contain the expected Windows archive and checksum."
        }
        $comparison = (Convert-ReleaseVersion $releaseVersion).CompareTo((Convert-ReleaseVersion $CurrentVersion))
        if ($Check) {
            if ($comparison -gt 0) {
                Write-Output "Agent Workboard $releaseVersion is available; installed version is $CurrentVersion."
            }
            else {
                Write-Output "Agent Workboard $CurrentVersion is up to date."
            }
            return
        }
        if ($comparison -le 0 -and -not $Force) {
            Write-Output "Agent Workboard $CurrentVersion is up to date."
            return
        }
        $archivePath = Join-Path $temporaryRoot $wantedArchive
        $checksumPath = Join-Path $temporaryRoot $wantedChecksum
        Invoke-WebRequest -Uri $archiveAsset[0].browser_download_url -Headers $headers -OutFile $archivePath -UseBasicParsing
        Invoke-WebRequest -Uri $checksumAsset[0].browser_download_url -Headers $headers -OutFile $checksumPath -UseBasicParsing
    }

    if (-not (Test-Path -LiteralPath $archivePath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $checksumPath -PathType Leaf)) {
        throw 'The update archive or checksum does not exist.'
    }
    $checksumText = Get-Content -Raw -LiteralPath $checksumPath
    $checksumMatch = [regex]::Match($checksumText, '(?im)^([0-9a-f]{64})\s+\*?(.+?)\s*$')
    if (-not $checksumMatch.Success -or
        [System.IO.Path]::GetFileName($checksumMatch.Groups[2].Value) -ine [System.IO.Path]::GetFileName($archivePath)) {
        throw 'The release checksum file is invalid.'
    }
    $expectedHash = $checksumMatch.Groups[1].Value.ToLowerInvariant()
    $actualHash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) {
        throw 'The update archive failed published SHA-256 verification.'
    }

    $releaseRoot = Join-Path $temporaryRoot 'release'
    Expand-Archive -LiteralPath $archivePath -DestinationPath $releaseRoot
    $manifest = Get-OwnedManifest $releaseRoot
    $releaseVersion = $manifest.version
    $comparison = (Convert-ReleaseVersion $releaseVersion).CompareTo((Convert-ReleaseVersion $CurrentVersion))
    if ($Check) {
        if ($comparison -gt 0) {
            Write-Output "Agent Workboard $releaseVersion is available; installed version is $CurrentVersion."
        }
        else {
            Write-Output "Agent Workboard $CurrentVersion is up to date."
        }
        return
    }
    if ($comparison -le 0 -and -not $Force) {
        Write-Output "Agent Workboard $CurrentVersion is up to date."
        return
    }

    if ($ParentProcessId -gt 0) {
        Wait-Process -Id $ParentProcessId -ErrorAction SilentlyContinue
    }
    Get-CimInstance Win32_Process -Filter "Name = 'workboard-daemon.exe'" -ErrorAction SilentlyContinue |
        Where-Object {
            $_.ExecutablePath -and
            [System.IO.Path]::GetFullPath($_.ExecutablePath) -ieq (Join-Path $InstallRoot 'workboard-daemon.exe')
        } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force }

    $installedManifestPath = Join-Path $InstallRoot 'installed-manifest.json'
    $preserveNoPath = $false
    if (Test-Path -LiteralPath $installedManifestPath -PathType Leaf) {
        $installedManifest = Get-Content -Raw -LiteralPath $installedManifestPath | ConvertFrom-Json
        $preserveNoPath = $installedManifest.pathAdded -ne $true
    }
    if ($preserveNoPath) {
        & (Join-Path $releaseRoot 'Install.ps1') -InstallRoot $InstallRoot -NoPath
    }
    else {
        & (Join-Path $releaseRoot 'Install.ps1') -InstallRoot $InstallRoot
    }
    Write-Output "Updated Agent Workboard from $CurrentVersion to $releaseVersion."
}
finally {
    if (Test-Path -LiteralPath $temporaryRoot -PathType Container) {
        $resolvedTemp = [System.IO.Path]::GetFullPath($env:TEMP).TrimEnd('\') + '\'
        $resolvedRoot = [System.IO.Path]::GetFullPath($temporaryRoot)
        if ($resolvedRoot.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase) -and
            [System.IO.Path]::GetFileName($resolvedRoot).StartsWith('agent-workboard-update-')) {
            Remove-Item -LiteralPath $resolvedRoot -Recurse -Force
        }
    }
}
