# Installing workgraph on Windows

Download a prebuilt `wg.exe` from a GitHub Release -- no Rust toolchain, no
build dependencies. This page covers the happy path; for the full "what's
different on Windows" reference see `docs/README-windows.md`.

## One-line install

Open PowerShell and run:

```powershell
irm https://github.com/njt/workgraph/releases/latest/download/install.ps1 | iex
```

That script:

1. Detects your CPU (x86_64 or ARM64) and pulls the matching release zip.
2. Verifies the SHA256 of `wg.exe` against the archive's `SHA256SUMS`.
3. Copies `wg.exe` to `%USERPROFILE%\workgraph\bin\`.
4. Adds that directory to your user `PATH` (persistent, no admin).
5. Runs `wg doctor` so you can see at a glance what else you need.

Everything stays under `%USERPROFILE%`. No elevation, no registry edits
outside `HKCU`, no services registered.

## Environment overrides

Useful if you want to pin a version or target an unusual setup:

```powershell
# Pin to a specific release
$env:WG_VERSION = "v0.1.0+win.1"

# Install somewhere other than %USERPROFILE%\workgraph\bin
$env:WG_INSTALL_DIR = "C:\tools\wg"

# Pull from a different fork
$env:WG_REPO = "graphwork/workgraph"

irm https://github.com/njt/workgraph/releases/latest/download/install.ps1 | iex
```

## What `wg.exe` doesn't bring with it

Three runtime dependencies the installer deliberately leaves alone --
they're shared with lots of other tools and you probably want to install
them yourself:

| Dependency | What it provides | How to get it |
|---|---|---|
| **Git for Windows** | `bash.exe`, GNU `cat` / `tee` / `timeout`, `git.exe` | `winget install Git.Git` |
| **`claude` CLI** | the actual model runner | comes with [Claude Code](https://claude.ai/download); or `claude setup-token` for a headless token |
| **Claude credentials** | OAuth token or API key | `claude login` once, or set `CLAUDE_CODE_OAUTH_TOKEN` |

Run `wg doctor` after install -- it checks each of these and tells you the
exact command to fix anything missing.

## First run

Once `wg.exe` is on PATH and the runtime deps are in place:

```powershell
cd C:\path\to\your\project
wg init
wg add "First task" -d "Hello world"
wg service start
```

Then `wg tui` to watch it run, or `wg status` for a one-screen overview.

## Manual install (without the script)

If you'd rather not `iex` a remote script, the release page has everything
you need:

1. Go to `https://github.com/njt/workgraph/releases/latest`.
2. Download `wg-<version>-<arch>-pc-windows-msvc.zip` matching your CPU.
3. Extract. Verify the SHA256 listed in `SHA256SUMS` matches the `wg.exe`
   you got (`Get-FileHash -Algorithm SHA256 wg.exe`).
4. Put `wg.exe` somewhere on your `PATH`. `%USERPROFILE%\workgraph\bin`
   is the script's convention if you don't have a preference.

## Uninstall

```powershell
Remove-Item -Recurse -Force $env:USERPROFILE\workgraph\bin
# If you want to remove it from PATH too:
$p = [Environment]::GetEnvironmentVariable('Path', 'User')
$p = ($p -split ';' | Where-Object { $_ -notmatch 'workgraph\\bin$' }) -join ';'
[Environment]::SetEnvironmentVariable('Path', $p, 'User')
```

Your project's `.workgraph/` directory is left alone -- remove by hand if
you want to fully reset.

## Upgrades

Re-run the one-liner. The installer overwrites `wg.exe` in place,
refusing only if a daemon is actively running from it -- in which case
`wg service stop` first.
