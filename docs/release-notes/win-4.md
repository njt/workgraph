# v0.1.0-preview.1+win.4

Aligns `wg doctor` with the bash resolver introduced in +win.3. Previously, doctor checked bash by running `bash --version` via PATH lookup and `where bash`, which on a stock Windows install found the WSL shim and reported "not Git-for-Windows" — even though the runtime resolver correctly picked Git bash from a well-known install location. Doctor now uses the same `platform_bash::resolve_bash` function that the rest of workgraph uses, shows which resolution branch matched (config, env, well-known, PATH, fallback), and runs `--version` on the resolved path. The diagnostic output now accurately reflects what `wg` will actually do at runtime.

**What to test:** Run `wg doctor` and confirm the bash diagnostic shows the correct Git-for-Windows bash path and resolution source. Compare with `wg service start` behavior — the two should agree.

**Known issues:** ARM64 builds still deferred.
