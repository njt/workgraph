# v0.1.0-preview.1+win.2

Patch release fixing the GitHub Actions release workflow. The aarch64-pc-windows-msvc target is deferred from CI — CMake's `enable_language(ASM_NASM)` fires before `OPENSSL_NO_ASM` takes effect, and NASM is not available on the windows-11-arm runner. ARM64 users should run the x86_64 binary under Windows' built-in x64 emulation. The x86_64 build itself was succeeding but failing at the archive step: `Out-File SHA256SUMS` opened the file while the pipeline was still enumerating, causing a file-lock collision. The hash computation is now fully materialised before writing.

**What to test:** Download the release zip from GitHub Releases and verify `SHA256SUMS` matches the included `wg.exe`.

**Known issues:** ARM64 native binary still deferred. The WSL bash shim resolution issue from +win.1 remains.
