# Windows release process

The current Windows artifact is an unsigned dogfood candidate. Run the release build on 64-bit Windows from
a clean checkout whose `HEAD` is the intended source revision:

```powershell
.\scripts\New-WindowsRelease.ps1
.\scripts\Test-WindowsRelease.ps1 .\artifacts\release\agent-workboard-0.1.0-x86_64-pc-windows-msvc.zip
```

The build uses `Cargo.lock`, the pinned Rust toolchain, release-mode MSVC binaries, sorted archive entries and
the source commit time for ZIP metadata. It emits the ZIP, a SHA-256 file, CycloneDX SBOM, and unsigned SLSA
provenance. Build a second time into another empty output directory and compare ZIP hashes when qualifying a
reproducible release environment.

The release test installs into an isolated per-user-style root, runs diagnostics and workflow smoke checks
without the source checkout or Node.js on `PATH`, starts the daemon on loopback, verifies provider fixtures
were not changed, and uninstalls. Run `Test-ProviderCompatibility.ps1` separately against authenticated
upstream Claude and Codex installations.

The hook ingestion boundary has a `cargo-fuzz` target because it accepts provider-controlled JSON on every
managed lifecycle event:

```powershell
cargo +nightly fuzz run hook_input -- -max_total_time=60
```

Before a public signed release, the owner must provide an Authenticode code-signing identity backed by a
protected key, the certificate chain, an RFC 3161 timestamp service, and the public publisher identity. Sign
both executables and both PowerShell scripts before creating the manifest, SBOM, provenance, checksums and
archive; verify signatures on a clean machine. Signing identity and contract freeze remain owner-acceptance
gates and are not simulated by the unsigned pipeline.
