[CmdletBinding()]
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\artifacts\release'),
    [switch]$AllowDirty
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
$target = 'x86_64-pc-windows-msvc'
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Write-Utf8File([string]$Path, [string]$Contents) {
    [System.IO.File]::WriteAllText($Path, $Contents.TrimEnd() + "`n", $utf8)
}

function Get-RelativeFiles([string]$Root) {
    Get-ChildItem -LiteralPath $Root -File -Recurse |
        ForEach-Object { $_.FullName.Substring($Root.Length).TrimStart('\').Replace('\', '/') } |
        Sort-Object
}

if (-not $IsWindows -and $PSVersionTable.PSEdition -eq 'Core') {
    throw 'The Windows release must be built on Windows.'
}

Push-Location $repositoryRoot
try {
    if (-not $AllowDirty) {
        $status = git status --porcelain=v1 --untracked-files=all
        if ($LASTEXITCODE -ne 0 -or $status) {
            throw 'The Windows release requires a clean checkout.'
        }
    }

    $workspaceManifest = Get-Content -Raw -LiteralPath (Join-Path $repositoryRoot 'Cargo.toml')
    $versionMatch = [regex]::Match($workspaceManifest, '(?m)^version = "([^"]+)"$')
    if (-not $versionMatch.Success) {
        throw 'The workspace package version was not found.'
    }
    $version = $versionMatch.Groups[1].Value
    $commit = (git rev-parse HEAD).Trim()
    $commitTime = [DateTimeOffset]::FromUnixTimeSeconds([long](git show -s --format=%ct HEAD)).UtcDateTime
    $rustVersion = (rustc --version).Trim()
    $hostLine = rustc -vV | Where-Object { $_ -like 'host:*' }
    $hostTarget = ($hostLine -split ':', 2)[1].Trim()
    if ($hostTarget -ne $target) {
        throw "The release host target is $hostTarget; expected $target."
    }

    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    $artifactName = "agent-workboard-$version-$target"
    $archive = Join-Path $OutputDirectory "$artifactName.zip"
    if (Test-Path -LiteralPath $archive) {
        throw "Release archive already exists: $archive"
    }
    $staging = Join-Path $OutputDirectory ('.staging-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $staging | Out-Null
    try {
        $env:CARGO_BUILD_JOBS = '1'
        cargo build --locked --release -p workboard-cli -p workboard-daemon
        if ($LASTEXITCODE -ne 0) {
            throw 'The release build failed.'
        }

        Copy-Item -LiteralPath (Join-Path $repositoryRoot 'target\release\workboard.exe') -Destination $staging
        Copy-Item -LiteralPath (Join-Path $repositoryRoot 'target\release\workboard-daemon.exe') -Destination $staging
        Copy-Item -LiteralPath (Join-Path $repositoryRoot 'packaging\windows\Install.ps1') -Destination $staging
        Copy-Item -LiteralPath (Join-Path $repositoryRoot 'packaging\windows\Uninstall.ps1') -Destination $staging
        Copy-Item -LiteralPath (Join-Path $repositoryRoot 'docs\operations\WINDOWS.md') -Destination $staging
        Copy-Item -LiteralPath (Join-Path $repositoryRoot 'docs\operations\RELEASING.md') -Destination $staging
        Copy-Item -LiteralPath (Join-Path $repositoryRoot 'README.md') -Destination $staging
        Copy-Item -LiteralPath (Join-Path $repositoryRoot 'LICENSE') -Destination $staging

        $lock = Get-Content -Raw -LiteralPath (Join-Path $repositoryRoot 'Cargo.lock')
        $lockedPackages = @([regex]::Split($lock, '(?m)^\[\[package\]\]\s*$') | Select-Object -Skip 1 | ForEach-Object {
            $name = [regex]::Match($_, '(?m)^name = "([^"]+)"$').Groups[1].Value
            $packageVersion = [regex]::Match($_, '(?m)^version = "([^"]+)"$').Groups[1].Value
            $source = [regex]::Match($_, '(?m)^source = "([^"]+)"$').Groups[1].Value
            $checksum = [regex]::Match($_, '(?m)^checksum = "([0-9a-f]+)"$').Groups[1].Value
            [pscustomobject]@{
                name = $name
                version = $packageVersion
                source = $source
                checksum = $checksum
            }
        })
        $components = @($lockedPackages | Sort-Object name, version, source | ForEach-Object {
            $reference = "pkg:cargo/$([Uri]::EscapeDataString($_.name))@$([Uri]::EscapeDataString($_.version))"
            $component = [ordered]@{
                type = 'library'
                name = $_.name
                version = $_.version
                'bom-ref' = $reference
                purl = $reference
            }
            if ($_.checksum) {
                $component.hashes = @([ordered]@{ alg = 'SHA-256'; content = $_.checksum })
            }
            [pscustomobject]$component
        })
        $sbom = [ordered]@{
            bomFormat = 'CycloneDX'
            specVersion = '1.5'
            version = 1
            metadata = [ordered]@{
                component = [ordered]@{
                    type = 'application'
                    name = 'agent-workboard'
                    version = $version
                    'bom-ref' = "pkg:cargo/agent-workboard@$version"
                }
                tools = [ordered]@{
                    components = @([ordered]@{
                        type = 'application'
                        name = 'cargo'
                        version = (cargo --version).Trim()
                    })
                }
            }
            components = $components
        }
        $sbomPath = Join-Path $staging 'SBOM.cdx.json'
        Write-Utf8File $sbomPath ($sbom | ConvertTo-Json -Depth 20)

        $subjectFiles = Get-RelativeFiles $staging
        $subjects = @($subjectFiles | ForEach-Object {
            $hash = (Get-FileHash -LiteralPath (Join-Path $staging $_) -Algorithm SHA256).Hash.ToLowerInvariant()
            [pscustomobject][ordered]@{ name = $_; digest = [ordered]@{ sha256 = $hash } }
        })
        $provenance = [ordered]@{
            '_type' = 'https://in-toto.io/Statement/v1'
            subject = $subjects
            predicateType = 'https://slsa.dev/provenance/v1'
            predicate = [ordered]@{
                buildDefinition = [ordered]@{
                    buildType = 'https://github.com/tommyseery/agent-workboard/windows-zip@v1'
                    externalParameters = [ordered]@{
                        target = $target
                        profile = 'release'
                        lockedDependencies = $true
                        rustToolchain = $rustVersion
                    }
                    internalParameters = [ordered]@{}
                    resolvedDependencies = @([ordered]@{
                        uri = "git+https://github.com/tommyseery/agent-workboard@$commit"
                        digest = [ordered]@{ gitCommit = $commit }
                    })
                }
                runDetails = [ordered]@{
                    builder = [ordered]@{ id = 'agent-workboard/scripts/New-WindowsRelease.ps1@v1' }
                    metadata = [ordered]@{ invocationId = $commit }
                    byproducts = @()
                }
            }
        }
        $provenancePath = Join-Path $staging 'provenance.json'
        Write-Utf8File $provenancePath ($provenance | ConvertTo-Json -Depth 20)

        $manifestFiles = @(Get-RelativeFiles $staging | ForEach-Object {
            [pscustomobject][ordered]@{
                path = $_
                sha256 = (Get-FileHash -LiteralPath (Join-Path $staging $_) -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        })
        $releaseManifest = [ordered]@{
            schemaVersion = 1
            owner = 'agent-workboard/windows-release-v1'
            version = $version
            target = $target
            files = $manifestFiles
        }
        Write-Utf8File (Join-Path $staging 'release-manifest.json') ($releaseManifest | ConvertTo-Json -Depth 10)

        $checksumLines = @(Get-RelativeFiles $staging | ForEach-Object {
            $hash = (Get-FileHash -LiteralPath (Join-Path $staging $_) -Algorithm SHA256).Hash.ToLowerInvariant()
            "$hash  $_"
        })
        Write-Utf8File (Join-Path $staging 'SHA256SUMS') ($checksumLines -join "`n")

        Add-Type -AssemblyName System.IO.Compression
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $archiveStream = [System.IO.File]::Open($archive, [System.IO.FileMode]::CreateNew)
        try {
            $zip = New-Object System.IO.Compression.ZipArchive(
                $archiveStream,
                [System.IO.Compression.ZipArchiveMode]::Create,
                $false
            )
            try {
                foreach ($relative in Get-RelativeFiles $staging) {
                    $entry = $zip.CreateEntry($relative, [System.IO.Compression.CompressionLevel]::Optimal)
                    $entry.LastWriteTime = [DateTimeOffset]$commitTime
                    $entryStream = $entry.Open()
                    try {
                        $bytes = [System.IO.File]::ReadAllBytes((Join-Path $staging $relative))
                        $entryStream.Write($bytes, 0, $bytes.Length)
                    }
                    finally {
                        $entryStream.Dispose()
                    }
                }
            }
            finally {
                $zip.Dispose()
            }
        }
        finally {
            $archiveStream.Dispose()
        }

        $archiveHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        Write-Utf8File "$archive.sha256" "$archiveHash  $([System.IO.Path]::GetFileName($archive))"
        Copy-Item -LiteralPath $sbomPath -Destination (Join-Path $OutputDirectory "$artifactName.cdx.json")
        Copy-Item -LiteralPath $provenancePath -Destination (Join-Path $OutputDirectory "$artifactName.provenance.json")
        Write-Output "Built $archive"
        Write-Output "SHA256 $archiveHash"
    }
    finally {
        $resolvedOutput = [System.IO.Path]::GetFullPath($OutputDirectory).TrimEnd('\') + '\'
        $resolvedStaging = [System.IO.Path]::GetFullPath($staging)
        if ($resolvedStaging.StartsWith($resolvedOutput, [StringComparison]::OrdinalIgnoreCase) -and
            [System.IO.Path]::GetFileName($resolvedStaging).StartsWith('.staging-')) {
            Remove-Item -LiteralPath $resolvedStaging -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
finally {
    Pop-Location
}
