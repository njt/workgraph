# v0.1.0-preview.1+win.5

Two small UX improvements surfaced by tester feedback. First, `wg doctor` described the Windows `timeout.exe` conflict using internal module names ("wg's own cross-platform timeout helper"); this is rewritten in plain language explaining that scripts calling `timeout` will get Windows' interactive `timeout.exe` instead of the Unix-style delay, and how to fix it. Second, `install.ps1` unconditionally told users to restart their terminal for PATH changes even when the install directory was already on PATH and no change was made; it now tracks whether PATH was actually modified and skips the advice when unnecessary.

**What to test:** Run `wg doctor` and read the timeout-related info line — it should make sense without knowledge of wg internals. Run `install.ps1` on a machine where wg is already installed and on PATH — it should not tell you to restart your terminal.

**Known issues:** ARM64 builds still deferred.
