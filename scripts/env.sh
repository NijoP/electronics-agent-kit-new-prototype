# Source this in Git Bash before building on Windows: `source scripts/env.sh`
# Adds the Rust toolchain and a MinGW-w64 GCC (WinLibs) to PATH. No Python involved.
export PATH="$HOME/.cargo/bin:$PATH"
for d in "$LOCALAPPDATA"/Microsoft/WinGet/Packages/BrechtSanders.WinLibs*/mingw64/bin; do
  [ -d "$d" ] && export PATH="$d:$PATH"
done
