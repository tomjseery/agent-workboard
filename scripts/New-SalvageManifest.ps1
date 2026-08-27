param(
    [Parameter(Mandatory)]
    [string]$Source,
    [Parameter(Mandatory)]
    [string]$Destination
)

$ErrorActionPreference = 'Stop'

$sourcePath = (Resolve-Path -LiteralPath $Source).Path
$destinationPath = [System.IO.Path]::GetFullPath($Destination)
$tracked = @(& git -C $sourcePath ls-files)
$untracked = @(& git -C $sourcePath ls-files --others --exclude-standard)
$status = @{}

foreach ($line in @(& git -C $sourcePath status --porcelain=v1 --untracked-files=all)) {
    $status[$line.Substring(3).Replace('\', '/')] = $line.Substring(0, 2)
}

function Get-SalvageDecision([string]$path) {
    if ($path -match '^crates/context-(core|adapter|adapter-claude|adapter-codex)/') {
        return @('keep-adapt', 'provider-neutral core or native adapter')
    }

    if ($path -match '^crates/context-application/src/(caller|error|git|hooks|integration|provider|resume)\.rs$') {
        return @('keep-adapt', 'native launch or integration boundary')
    }

    if ($path -match '^crates/contextd/src/(client|endpoint|error|protocol|server|watcher)\.rs$') {
        return @('keep-adapt', 'daemon IPC or watcher foundation')
    }

    if ($path -match '^crates/context-application/src/storage/(compatibility|diagnostics|discovery|provider|schema|write)\.rs$') {
        return @('redesign', 'storage primitive to extract from catalogue schema')
    }

    if ($path -match '^crates/context-application/' -or $path -match '^crates/contextctl/' -or $path -match '^crates/contextd/') {
        return @('redesign', 'application, command, storage, or daemon composition')
    }

    if ($path -in @('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.gitattributes', '.gitignore')) {
        return @('redesign', 'workspace identity and build baseline')
    }

    return @('drop', 'legacy product, graphical client, release surface, plan, or documentation')
}

$records = foreach ($path in @($tracked + $untracked | Sort-Object -Unique)) {
    $absolutePath = Join-Path $sourcePath $path
    $isTracked = $tracked -contains $path
    $headObject = if ($isTracked) { (& git -C $sourcePath rev-parse "HEAD:$path").Trim() } else { '-' }
    $workingHash = if (Test-Path -LiteralPath $absolutePath -PathType Leaf) {
        (Get-FileHash -LiteralPath $absolutePath -Algorithm SHA256).Hash.ToLowerInvariant()
    } else {
        '-'
    }
    $workingState = if ($status.ContainsKey($path)) { $status[$path] } elseif ($isTracked) { '  ' } else { '??' }
    $decision = Get-SalvageDecision $path

    [pscustomobject]@{
        Path = $path.Replace('\', '/')
        State = $workingState
        HeadObject = $headObject
        WorkingSha256 = $workingHash
        Decision = $decision[0]
        Reason = $decision[1]
    }
}

$header = "path`tstate`thead_object_sha1`tworking_sha256`tdecision`treason"
$rows = $records | ForEach-Object {
    "$($_.Path)`t$($_.State)`t$($_.HeadObject)`t$($_.WorkingSha256)`t$($_.Decision)`t$($_.Reason)"
}

[System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($destinationPath)) | Out-Null
[System.IO.File]::WriteAllLines($destinationPath, @($header) + $rows, [System.Text.UTF8Encoding]::new($false))

