#Requires -Version 5.1
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$DaemonToo,
    [switch]$NoLaunch
)

$ErrorActionPreference = 'Stop'

$appRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent (Split-Path -Parent $appRoot)
$builtDesktop = Join-Path $repoRoot 'target\release\workboard-desktop.exe'
$builtDaemon = Join-Path $repoRoot 'target\release\workboard.exe'
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\Agent Workboard Desktop'
$installedDesktop = Join-Path $installDir 'Agent Workboard.exe'
$installedDaemon = Join-Path $installDir 'workboard.exe'
$shortcut = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Agent Workboard.lnk'
$webViewDataDir = 'dev.agentworkboard.desktop'

function Write-Step([string]$text) { Write-Host "==> $text" -ForegroundColor Cyan }
function Write-Detail([string]$text) { Write-Host "    $text" }

function Invoke-Native([string]$What, [scriptblock]$Command) {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $Command }
    finally { $ErrorActionPreference = $previous }
    if ($LASTEXITCODE -ne 0) {
        throw "$What failed with exit code $LASTEXITCODE. The installed binaries were left untouched."
    }
}

function Get-ProcessDescendant([int[]]$Roots) {
    $childrenByParent = @{}
    foreach ($process in Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId) {
        $parent = [int]$process.ParentProcessId
        if (-not $childrenByParent.ContainsKey($parent)) {
            $childrenByParent[$parent] = New-Object System.Collections.Generic.List[int]
        }
        $childrenByParent[$parent].Add([int]$process.ProcessId)
    }
    $found = New-Object System.Collections.Generic.List[int]
    $frontier = New-Object System.Collections.Generic.List[int]
    $frontier.AddRange($Roots)
    while ($frontier.Count -gt 0) {
        $next = New-Object System.Collections.Generic.List[int]
        foreach ($parent in $frontier) {
            if (-not $childrenByParent.ContainsKey($parent)) { continue }
            foreach ($child in $childrenByParent[$parent]) {
                if ($Roots -contains $child -or $found.Contains($child)) { continue }
                $found.Add($child)
                $next.Add($child)
            }
        }
        $frontier = $next
    }
    $found.ToArray()
}

function Get-OwnedProcess([string]$Name, [string[]]$Paths) {
    Get-CimInstance Win32_Process -Filter "Name = '$Name'" |
        Where-Object { $_.ExecutablePath -and ($Paths -contains $_.ExecutablePath) } |
        ForEach-Object { [int]$_.ProcessId }
}

function Get-OrphanedWebView {
    Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" |
        Where-Object {
            $_.CommandLine -like "*$webViewDataDir*" -or
            $_.CommandLine -like '*--webview-exe-name=Agent Workboard.exe*'
        } |
        ForEach-Object { [int]$_.ProcessId }
}

function Stop-Tracked([int[]]$Ids, [string]$What) {
    if ($Ids.Count -eq 0) { return }
    Write-Detail "stopping $($Ids.Count) $What ($($Ids -join ', '))"
    foreach ($id in $Ids) {
        try { Stop-Process -Id $id -Force -ErrorAction Stop }
        catch [Microsoft.PowerShell.Commands.ProcessCommandException] { }
    }
}

function Stop-Workboard {
    [int[]]$appRoots = @()
    $appRoots += @(Get-OwnedProcess -Name 'Agent Workboard.exe' -Paths @($installedDesktop))
    $appRoots += @(Get-OwnedProcess -Name 'workboard-desktop.exe' -Paths @($builtDesktop))
    [int[]]$daemons = @(Get-OwnedProcess -Name 'workboard.exe' -Paths @($installedDaemon, $builtDaemon))

    [int[]]$children = @()
    if ($appRoots.Count -gt 0) { $children = @(Get-ProcessDescendant -Roots $appRoots) }
    [int[]]$orphans = @(Get-OrphanedWebView | Where-Object { $children -notcontains $_ })

    if ($appRoots.Count -eq 0 -and $daemons.Count -eq 0 -and $orphans.Count -eq 0) {
        Write-Detail 'nothing running'
        return
    }
    Stop-Tracked -Ids $children -What 'child process(es) of the app'
    Stop-Tracked -Ids $orphans -What 'orphaned Agent Workboard WebView2 process(es)'
    Stop-Tracked -Ids $daemons -What 'workboard daemon process(es)'
    Stop-Tracked -Ids $appRoots -What 'Agent Workboard window process(es)'
}

function Wait-Writable([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return }
    for ($attempt = 1; $attempt -le 20; $attempt++) {
        try {
            $stream = [System.IO.File]::Open($Path, 'Open', 'Write', 'None')
            $stream.Close()
            return
        }
        catch { Start-Sleep -Milliseconds 250 }
    }
    throw "$Path is still locked. Close every Agent Workboard window and run this again."
}

function Install-Binary([string]$Source, [string]$Destination) {
    Wait-Writable -Path $Destination
    for ($attempt = 1; $attempt -le 20; $attempt++) {
        try {
            Copy-Item -LiteralPath $Source -Destination $Destination -Force -ErrorAction Stop
            return
        }
        catch {
            if ($attempt -eq 20) { throw }
            Start-Sleep -Milliseconds 250
        }
    }
}

function Test-SameContent([string]$Left, [string]$Right) {
    if (-not (Test-Path -LiteralPath $Right)) { return $false }
    return (Get-FileHash -LiteralPath $Left -Algorithm SHA256).Hash -eq
        (Get-FileHash -LiteralPath $Right -Algorithm SHA256).Hash
}

function Set-DesktopShortcut {
    $shell = New-Object -ComObject WScript.Shell
    try {
        if (Test-Path -LiteralPath $shortcut) {
            $existing = $shell.CreateShortcut($shortcut)
            if ($existing.TargetPath -eq $installedDesktop) {
                Write-Detail 'desktop shortcut already correct'
                return
            }
            Write-Detail "desktop shortcut pointed at $($existing.TargetPath); repointing it"
        }
        else {
            Write-Detail 'creating the desktop shortcut'
        }
        $link = $shell.CreateShortcut($shortcut)
        $link.TargetPath = $installedDesktop
        $link.WorkingDirectory = $installDir
        $link.IconLocation = $installedDesktop
        $link.Description = 'Agent Workboard'
        $link.Save()
    }
    finally { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($shell) }
}

Write-Step 'Stopping Agent Workboard'
Stop-Workboard

if ($SkipBuild) {
    Write-Step 'Skipping the build (-SkipBuild)'
    if (-not (Test-Path -LiteralPath $builtDesktop)) {
        throw "-SkipBuild was given but there is no build at $builtDesktop. Run without -SkipBuild."
    }
}
else {
    if ($DaemonToo) {
        Write-Step 'Building workboard.exe (cargo build --release -p workboard-cli)'
        Push-Location $repoRoot
        try { Invoke-Native 'cargo build --release -p workboard-cli' { cargo build --release -p workboard-cli } }
        finally { Pop-Location }
    }

    Write-Step 'Building the desktop client (npm run app)'
    Push-Location $appRoot
    try { Invoke-Native 'npm run app' { npm run app } }
    finally { Pop-Location }

    if (-not (Test-Path -LiteralPath $builtDesktop)) {
        throw "The build reported success but produced no $builtDesktop. Refusing to report an install."
    }
}

if (-not (Test-Path -LiteralPath $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
}

Write-Step 'Installing'
Install-Binary -Source $builtDesktop -Destination $installedDesktop
Write-Detail "Agent Workboard.exe <- $builtDesktop"

if (-not (Test-Path -LiteralPath $builtDaemon)) {
    if (-not (Test-Path -LiteralPath $installedDaemon)) {
        throw "There is no daemon at $builtDaemon and none installed. Run this with -DaemonToo."
    }
    Write-Detail 'workboard.exe not built in this tree; kept the installed one'
}
elseif (Test-SameContent -Left $builtDaemon -Right $installedDaemon) {
    Write-Detail 'workboard.exe unchanged; kept the installed one'
}
else {
    Install-Binary -Source $builtDaemon -Destination $installedDaemon
    Write-Detail "workboard.exe <- $builtDaemon"
}

Write-Step 'Shortcut'
Set-DesktopShortcut

Write-Step 'Installed'
Get-ChildItem -LiteralPath $installDir -Filter '*.exe' |
    ForEach-Object { Write-Detail "$($_.Name)  $($_.LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss'))" }

if ($NoLaunch) {
    Write-Step 'Not launching (-NoLaunch)'
}
else {
    Write-Step 'Launching Agent Workboard'
    Start-Process -FilePath $installedDesktop -WorkingDirectory $installDir | Out-Null
}
