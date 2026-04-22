# install.ps1 -- fetch and install a prebuilt `wg.exe` on Windows.
#
# One-liner:
#   irm https://github.com/<owner>/workgraph/releases/latest/download/install.ps1 | iex
#
# What it does:
#   1. Detects architecture (x86_64 or arm64).
#   2. Downloads the matching release asset from GitHub.
#   3. Verifies the SHA256SUMS included in the archive.
#   4. Extracts `wg.exe` into `$env:USERPROFILE\workgraph\bin`.
#   5. Adds that dir to the user PATH (persistent, HKCU).
#   6. Prints `wg doctor` to confirm.
#
# What it does NOT do:
#   - Install Git for Windows, `claude` CLI, or any other runtime dep.
#     Run `wg doctor` after install; it'll tell you what else to grab.
#   - Elevate to Administrator. All state goes under %USERPROFILE%.
#
# Override behavior via environment variables:
#   WG_REPO           "<owner>/workgraph"     # default: njt/workgraph
#   WG_VERSION        "v0.1.0+win.1"          # default: latest release
#   WG_INSTALL_DIR    "C:\path\to\bin"        # default: %USERPROFILE%\workgraph\bin

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Write-Info($msg)    { Write-Host "[install] $msg" -ForegroundColor Cyan }
function Write-Ok($msg)      { Write-Host "[install] $msg" -ForegroundColor Green }
function Write-WarnMsg($msg) { Write-Host "[install] $msg" -ForegroundColor Yellow }
function Fail($msg)          { Write-Host "[install] ERROR: $msg" -ForegroundColor Red; exit 1 }

# --- 1. detect arch ---------------------------------------------------

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    'AMD64' { 'x86_64-pc-windows-msvc' }
    'ARM64' { 'aarch64-pc-windows-msvc' }
    default { Fail "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
}
Write-Info "Architecture: $arch"

# --- 2. resolve release -----------------------------------------------

$repo = if ($env:WG_REPO) { $env:WG_REPO } else { 'njt/workgraph' }
$tag  = $env:WG_VERSION

if (-not $tag) {
    Write-Info "Resolving latest release from $repo ..."
    $api = "https://api.github.com/repos/$repo/releases/latest"
    try {
        $tag = (Invoke-RestMethod -Uri $api -Headers @{ 'User-Agent' = 'wg-installer' }).tag_name
    } catch {
        Fail "Could not resolve latest release. Set WG_VERSION=vX.Y.Z explicitly or check network access to api.github.com. ($_)"
    }
}
Write-Info "Release: $tag"

$version = $tag -replace '^v',''
$asset   = "wg-$version-$arch.zip"
$url     = "https://github.com/$repo/releases/download/$tag/$asset"

# --- 3. download + verify ---------------------------------------------

$tmp = New-Item -Type Directory -Path (Join-Path $env:TEMP "wg-install-$([guid]::NewGuid())") -Force
$zip = Join-Path $tmp $asset

Write-Info "Downloading $url"
try {
    Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
} catch {
    Fail "Download failed: $_"
}

Write-Info "Extracting ..."
Expand-Archive -Path $zip -DestinationPath $tmp -Force
$staging = Join-Path $tmp "wg-$version-$arch"
if (-not (Test-Path $staging)) {
    Fail "Expected staging dir $staging not found after extract"
}

# Verify sha256 of the extracted wg.exe against SHA256SUMS
$sumsPath = Join-Path $staging 'SHA256SUMS'
if (Test-Path $sumsPath) {
    $expected = (Get-Content $sumsPath | Where-Object { $_ -match '\s+wg\.exe$' } |
                 ForEach-Object { $_.Split()[0] }) | Select-Object -First 1
    if (-not $expected) {
        Write-WarnMsg "No wg.exe line in SHA256SUMS; skipping verify"
    } else {
        $actual = (Get-FileHash -Algorithm SHA256 (Join-Path $staging 'wg.exe')).Hash.ToLower()
        if ($expected -ne $actual) {
            Fail "SHA256 mismatch: expected $expected, got $actual"
        }
        Write-Ok "SHA256 verified"
    }
} else {
    Write-WarnMsg "No SHA256SUMS in archive; skipping verify"
}

# --- 4. install to %USERPROFILE%\workgraph\bin ------------------------

$installDir = if ($env:WG_INSTALL_DIR) { $env:WG_INSTALL_DIR } else {
    Join-Path $env:USERPROFILE 'workgraph\bin'
}
New-Item -Type Directory -Path $installDir -Force | Out-Null
$dest = Join-Path $installDir 'wg.exe'

# If a daemon is running from the existing wg.exe, refuse rather than
# truncate the binary mid-run.
Get-CimInstance Win32_Process -Filter "name='wg.exe'" |
    Where-Object { $_.CommandLine -match 'service daemon' -and $_.ExecutablePath -eq $dest } |
    ForEach-Object {
        Fail "Daemon is running from $dest (PID $($_.ProcessId)). Stop it with ``wg service stop`` first."
    }

Copy-Item (Join-Path $staging 'wg.exe') $dest -Force
Write-Ok "Installed: $dest"

# --- 5. PATH handling --------------------------------------------------

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notmatch [Regex]::Escape($installDir)) {
    $newPath = if ($userPath) { "$userPath;$installDir" } else { $installDir }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Ok "Added $installDir to user PATH (open a new terminal to pick it up)"
} else {
    Write-Info "$installDir already on user PATH"
}

# Also expose in the current shell so the `wg doctor` line below works.
if ($env:PATH -notmatch [Regex]::Escape($installDir)) {
    $env:PATH = "$env:PATH;$installDir"
}

# --- 6. sanity check --------------------------------------------------

Write-Host ""
Write-Info "Running ``wg doctor`` ..."
Write-Host ""
try {
    & $dest doctor
} catch {
    Write-WarnMsg "wg doctor exited non-zero -- review above. That's fine for info-only output; errors and warnings are listed at the end."
}

Write-Host ""
Write-Ok "Done. Restart your terminal so the updated PATH takes effect, then try: wg --help"
