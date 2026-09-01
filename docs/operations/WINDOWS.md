# Windows operations

This guide covers the unsigned Windows dogfood distribution. The command, database schema, and planning
document contracts are still pre-release interfaces and may change before v0 acceptance.

## Requirements

- 64-bit Windows 10 or later, PowerShell 7 or Windows PowerShell 5.1, Git, and Windows Terminal.
- Claude Code, Codex CLI, or both, already installed and authenticated for the Windows user.
- No Node.js or Agent Workboard source checkout is required by the packaged release. Provider installations
  may have their own runtime requirements.

## Verify and install

Download the Windows ZIP, its published SHA-256 digest, and its provenance from the same release. Verify the
archive before extracting it:

```powershell
(Get-FileHash .\agent-workboard-0.1.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256).Hash
```

The dogfood candidate is unsigned. Windows may mark downloaded files as untrusted; inspect the release
provenance and checksum before using `Unblock-File`. Extract the archive and run:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\Install.ps1
workboard --version
workboard diagnostics
```

The installer copies `workboard.exe`, the optional `workboard-daemon.exe`, documentation, SBOM, provenance,
and checksums to `%LOCALAPPDATA%\Programs\Agent Workboard`. It adds that directory to the current user's
`PATH`; a new terminal sees the change. Use `-InstallRoot` to choose another absolute per-user directory or
`-NoPath` to leave `PATH` unchanged.

Installation never writes to `.claude`, `.codex`, a repository skill directory, or provider-global hooks.
It installs no service, scheduled task, shell profile, or provider integration.

## First run and permissions

Create or select the external Git planning store, then register a code repository:

```powershell
workboard init --store "$HOME\agent-workboard-store"
workboard repository add C:\source\my-repository
```

Workboard needs ordinary user access to its database, planning store, registered repositories and managed
worktrees. A managed provider child also needs access to its assigned checkout and planning store under the
provider's own approval and sandbox policy. Workboard does not weaken that policy globally.

For each managed launch, Workboard creates a role-scoped configuration beside its database and sets
`CLAUDE_CONFIG_DIR` or `CODEX_HOME` only in that child process. The bundle contains only that role's skills,
hooks, assignment and a short-lived token. Provider credentials and supported provider settings are linked
or copied into the bundle for the child, never stored in SQLite, and removed when the managed session closes.
Provider transcripts remain so exact resume and recovery continue to work.

`workboard integration status|preview|remove` only finds and removes global residue written by an earlier
release. It cannot install an integration. Always preview removal and review the ownership result before
supplying its one-use confirmation token.

## Upgrade

Installed GitHub releases update in place with one command:

```powershell
workboard update
```

Use `workboard update --check` to check without installing or `workboard update --version 0.1.1` to select a
specific published version. The updater downloads the matching Windows archive and published checksum from
the project's GitHub Release, verifies the archive checksum and every release-manifest entry, stops only the
daemon running from the same installation, and reruns the per-user installer. The current CLI exits before
its executable is replaced.

The database, planning store, repository registrations, hierarchy, checkouts, managed-session bundles, and
provider homes are not package files and are preserved. You do not run `init`, re-add repositories, or
recreate work after an update. Source checkouts do not self-update; build a fresh candidate when developing
Workboard itself.

For a manual or pre-release upgrade, back up first, stop `workboard-daemon.exe` if it is running, extract the
newer archive, and run its installer:

```powershell
workboard backup "$HOME\workboard-backups\workboard-before-upgrade.sqlite"
.\Install.ps1
workboard diagnostics
```

The installer stages and replaces release-owned files in the same installation root. It does not modify the
database, planning store, provider homes, repositories, or transcripts. The first command using a newer
binary may migrate the database. Database migrations are one-way, so keep the pre-upgrade backup until the
new release has been accepted.

## Repair

Verify the archive again and rerun its `Install.ps1` with the same `InstallRoot`. This replaces only files in
the verified release manifest; it does not reset application data or install provider-global integration. Run
`workboard diagnostics` afterward. If storage health is not `ok`, preserve the database and diagnostics, then
restore the latest verified database backup instead of deleting or editing SQLite directly.

## Storage and backup

The default operational database and managed-session bundles are under:

```text
%LOCALAPPDATA%\Agent Workboard\Agent Workboard\data\
  workboard.sqlite
  managed-sessions\
```

`WORKBOARD_DATABASE` or global `--database` selects a different database. The default planning store is
`%USERPROFILE%\agent-workboard-store`, a normal Git repository whose actual path is chosen during `init`.
Code repositories, worktrees, and provider transcript homes remain independently owned.

A complete backup consists of:

1. A verified online SQLite backup made with `workboard backup <new-file>`.
2. The planning-store Git repository, including uncommitted changes and any commits not pushed elsewhere.
3. Any managed-session transcript directories under `managed-sessions` needed for exact native resume.

Do not copy a live WAL database file as the primary backup. `workboard backup` uses SQLite's backup API and
checks integrity, foreign keys and schema before publishing the destination. `workboard export <directory>`
exports the planning documents without `.git`; it is useful for inspection, but is not a replacement for the
Git planning-store backup.

## Recovery

Restore the planning store to its recorded path, restore the verified database and retained
`managed-sessions` directory, then run:

```powershell
workboard diagnostics
workboard recover --dry-run
workboard recover --yes
```

If the original path is unavailable, set `WORKBOARD_DATABASE` to the restored database. Repository or
worktree moves should be reconciled through Workboard rather than by editing SQLite. Recovery skips already
live sessions, recreates only checkouts that pass recorded Git preflight, and reports every unsafe entry as a
typed conflict. Use `--replace-unresumable` only after reviewing the dry run.

## Optional daemon

`workboard-daemon.exe` watches provider transcript roots and serialises refresh writes. It is optional; the
CLI remains usable without it. Start it for the current user in a separate hidden or background process:

```powershell
Start-Process workboard-daemon.exe -WindowStyle Hidden
```

The daemon binds only to a random loopback port and publishes an authenticated endpoint descriptor beside
the database. It is not installed for automatic startup. Stop the process before upgrading or uninstalling.

## Uninstall

Run the uninstall script from the extracted release, or from the installation directory:

```powershell
& "$env:LOCALAPPDATA\Programs\Agent Workboard\Uninstall.ps1"
```

Uninstall removes only release-owned files and the exact user `PATH` entry added by the installer. It leaves
the database, planning store, managed transcripts, provider homes, repositories, worktrees and unrelated
configuration intact. Use `-RemoveData` only after backing up and reviewing the displayed data path.

If `workboard integration status` reported residue from an earlier release, remove it through the preview and
confirmation flow before uninstalling the executable. The uninstaller deliberately does not edit provider
configuration.

## Troubleshooting

- `workboard` is not found: open a new terminal, or invoke the executable by its full install path. Run the
  installer without `-NoPath` if the user `PATH` entry was intentionally omitted.
- Windows blocks the unsigned executable: verify the release checksum and provenance, then use
  `Unblock-File` on the extracted release only if you trust that exact artifact.
- `capability_bundle_credential_missing`: launch the provider normally, authenticate it, close it, and retry
  the Workboard-managed launch.
- `capability_injection_unavailable`: run `workboard diagnostics` and `workboard integration status --tool
  claude` or `--tool codex`; provider policy may have disabled the required hook mechanism.
- A normal provider CLI sees Workboard skills: this is not expected. Run `workboard integration status` for
  that provider. Preview and remove only residue Workboard proves it owns; do not delete foreign files.
- Backup fails because the destination exists: choose a new filename. Workboard never overwrites a backup.
- `database schema ... is newer`: the installed executable is older than the database. Run `workboard update`;
  do not delete, edit, or downgrade the database.
- Recovery reports a checkout conflict: keep the dry-run output and resolve the recorded dirty, occupied,
  missing-branch or path evidence. Do not force recreation by editing the database.
- The daemon reports `daemon_already_running`: use the existing daemon or stop its process. Do not delete the
  lock while a daemon may still own it.

For a support report, attach `workboard diagnostics --json`, the exact error code and command, and the
release provenance. Diagnostics reports paths and capability state but does not include workflow tokens,
provider credentials, transcript content or planning document bodies.
