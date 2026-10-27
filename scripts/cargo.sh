#!/bin/sh
# Prefer a normal Rust installation; support this workspace's existing local toolchain.
set -eu
if ! command -v cargo >/dev/null 2>&1; then
    project_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
    if [ ! -x "$project_root/.cargo-home/bin/cargo" ]; then
        echo 'Cargo is unavailable. Install Rust via https://rustup.rs/.' >&2
        exit 1
    fi
    export CARGO_HOME="$project_root/.cargo-home"
    export RUSTUP_HOME="$project_root/.rustup-home"
    export PATH="$CARGO_HOME/bin:$PATH"
fi
exec cargo "$@"
