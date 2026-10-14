#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
if ! command -v cargo >/dev/null 2>&1; then
    toolchain_root="$(cd ../alfred-quickai/.tools && pwd)"
    export CARGO_HOME="$toolchain_root/cargo"
    export RUSTUP_HOME="$toolchain_root/rustup"
    export PATH="$CARGO_HOME/bin:$PATH"
fi
exec cargo "$@"
