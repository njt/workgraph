# v0.1.0-preview.1+win.3

Fixes the bash resolution problem flagged in +win.1. On a stock Windows install, `Command::new("bash")` resolves to the WSL shim at `C:\Windows\System32\bash.exe`, which cannot handle native Windows paths and silently breaks every wrapper script. A new `platform_bash` resolver uses an explicit precedence chain: config file override, `WG_BASH_PATH` env var, well-known Git-for-Windows install locations, then PATH (skipping the WSL shim). All internal `Command::new("bash")` call sites are routed through the resolver. A `[bash] path` option is added to `.workgraph/config.toml` for users who need to pin a specific bash. This was the most common failure mode reported by testers on +win.1 and +win.2.

**What to test:** `wg service start` on a machine where Git for Windows is installed but `Git\bin` is not on PATH. Verify agents spawn and execute tasks correctly. Check `wg doctor` still reports bash status accurately.

**Known issues:** `wg doctor` may show conflicting bash information — it still uses PATH lookup rather than the new resolver. ARM64 native binary deferred.
