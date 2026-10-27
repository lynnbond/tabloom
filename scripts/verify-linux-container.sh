#!/bin/sh
# Native Linux ARM64/x86_64 validation through Docker, with isolated Xvfb/clipboard.
set -eu
project_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
docker build -f scripts/linux-validation.Dockerfile -t tabloom-validation:local scripts
mkdir -p target/linux
# Never mount or access the host's real application data directory.
docker run --rm --init \
  -v "$project_root:/project:ro" -v "$project_root/target/linux:/build" \
  -v tabloom-linux-cargo:/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/build -e CARGO_BUILD_JOBS=1 \
  -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 \
  -e LIBGL_ALWAYS_SOFTWARE=1 -e XDG_RUNTIME_DIR=/tmp/tabloom-runtime \
  -w /project tabloom-validation:local bash -c '
    set -e
    mkdir -m 700 -p "$XDG_RUNTIME_DIR"
    rustc -Vv
    cargo fmt --all -- --check
    cargo check --all-targets --locked
    cargo test --locked
    cargo clippy --all-targets --all-features --locked -- -D warnings
    cargo build --locked --bins --examples
    xvfb-run -a /build/debug/examples/document_smoke /build/continuous-document.png
    for theme in A B C D; do xvfb-run -a /build/debug/examples/document_smoke /build/themes/$theme.png "$theme"; done
    xvfb-run -a python3 scripts/verify-linux-main.py
    xvfb-run -a /build/debug/examples/directory_smoke /build/directory-smoke.png
    xvfb-run -a python3 scripts/verify-native-persistence.py --screenshots /build/native-persistence
    xvfb-run -a python3 scripts/verify-native-files.py
    xvfb-run -a python3 scripts/verify-linux-clipboard.py
    xvfb-run -a /build/debug/examples/large_text_smoke /build/large-rust.png rust
  '
