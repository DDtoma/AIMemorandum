# AIMemorandum Windows desktop

## Local NSIS installer

Requires Rust/Cargo, PowerShell, and NSIS 3. From the repository root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows-installer.ps1
```

makensis comes from `MAKENSIS` or PATH. The builder compiles release by default; use `-SkipBuild` for an existing binary. `-Binary`, `-OutputDir`, and `-Architecture` support explicit local verification. Default output: `apps/desktop/dist/AIMemorandum-<version>-x86_64-setup.exe`, containing the executable, LICENSE, and THIRD_PARTY_NOTICES.md.

The installer is directory-only: current-user privileges, no registry, no shortcuts, and no automatic launch. Put `/D=` last to select an isolated directory. `Uninstall.exe` removes only known payload and the empty install directory:

```powershell
$isolated = Join-Path $PWD ".local/package-verify/windows/AIMemorandum"
& "apps/desktop/dist/AIMemorandum-0.1.0-x86_64-setup.exe" /S "/D=$isolated"
& (Join-Path $isolated "Uninstall.exe") /S
```

Run `$isolated/AIMemorandum.exe` only as a separate manual step. No distribution, signing/notarization, clean-system validation, or register → record → sync → reminder acceptance is claimed. Shared notice and test commands are in `docs/RUN.md`.
