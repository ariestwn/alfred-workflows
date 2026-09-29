#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -x .tools/cargo/bin/cargo ]; then
    export CARGO_HOME="$PWD/.tools/cargo"
    export RUSTUP_HOME="$PWD/.tools/rustup"
    export PATH="$CARGO_HOME/bin:$PATH"
fi
exec cargo "$@"

