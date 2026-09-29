#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Reuse a local Rust toolchain when Cargo isn't installed globally.
for kraely_tool_root in "$PWD/.tools" "$PWD/../alfred-quickai/.tools"; do
    if [ -x "$kraely_tool_root/cargo/bin/cargo" ]; then
        export CARGO_HOME="$kraely_tool_root/cargo"
        export RUSTUP_HOME="$kraely_tool_root/rustup"
        export PATH="$CARGO_HOME/bin:$PATH"
        break
    fi
done
exec cargo "$@"
