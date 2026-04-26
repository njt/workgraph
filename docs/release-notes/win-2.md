# v0.1.0-preview.1+win.2

Release-pipeline fix only — no changes to the `wg` binary itself. The +win.1 CI run exposed two problems: the ARM64 build failed because the BoringSSL dependency requires NASM during CMake configuration and NASM is unavailable on the `windows-11-arm` runner, and the x86_64 archive step raced when generating `SHA256SUMS` (the output file was opened while `Get-FileHash` was still enumerating the directory). This release defers native ARM64 builds until the NASM dependency is resolved and fixes the SHA256SUMS generation to materialise all hashes before writing the file.

**What to test:** Nothing new to test in the binary. Verify that the GitHub Release asset downloads correctly and the SHA256 checksum matches.

**Known issues:** ARM64 Windows builds are still deferred; use x86_64 under emulation.
