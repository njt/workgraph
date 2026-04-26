# v0.1.0-preview.1+win.5

Two small UX polish fixes from coworker testing. First, `wg doctor`'s informational line about `timeout(1)` previously referenced an internal module name ("cross-platform timeout helper") that meant nothing to users; it now explains in plain language that the Windows `timeout` command in PATH behaves differently from the Unix utility, and what to do if workflow scripts depend on it. Second, `install.ps1` unconditionally told users to restart their terminal for PATH changes even when the install directory was already on PATH and no change was made; it now tracks whether PATH was actually mutated and skips the advice when unnecessary.

**What to test:** Run `wg doctor` and read the timeout-related line for clarity. Run `install.ps1` twice — the second run should not prompt a terminal restart.

**Known issues:** ARM64 native binary still deferred.
