# v0.1.0-preview.1+win.3

Fixes the most common Windows setup failure: workgraph silently resolving `bash` to the WSL shim (`C:\Windows\System32\bash.exe`) instead of Git for Windows' bash. Git's default installer puts `git.exe` on PATH but not `bash.exe`, so `Command::new("bash")` found the WSL shim first — which cannot handle native Windows paths and broke every wrapper script without a useful error. A new `platform_bash` resolver now checks, in order: a `[bash] path` setting in `.workgraph/config.toml`, the `WG_BASH_PATH` environment variable, well-known Git for Windows install directories, and finally PATH (explicitly skipping the WSL shim). All internal `Command::new("bash")` call sites are routed through this resolver. Users who installed Git to a non-standard location can set `WG_BASH_PATH` or add `[bash] path = "..."` to their config.

**What to test:** Run `wg service start` on a stock Git for Windows install where `bash` on PATH is the WSL shim. Agents should now spawn correctly. If you have a non-standard Git install, verify that `WG_BASH_PATH` or the config override works.

**Known issues:** ARM64 builds still deferred.
