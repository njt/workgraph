# v0.1.0-preview.1+win.4

Aligns `wg doctor` with the bash resolver introduced in +win.3. Previously, doctor's bash check used a direct PATH lookup and `where bash`, which on most Windows machines found the WSL shim — contradicting the runtime behavior where the `platform_bash` resolver correctly picks Git-for-Windows bash from well-known locations. Doctor now runs the same resolver, shows which branch matched (config / env / well-known / PATH / fallback), and invokes `--version` on the resolved path. The raw PATH entry is kept as a separate informational line for debugging.

**What to test:** Run `wg doctor` and verify the bash section reports the same bash binary that agents actually use. Compare with +win.3 output if available.

**Known issues:** ARM64 native binary still deferred.
