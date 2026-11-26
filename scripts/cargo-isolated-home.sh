#!/bin/sh
# Run cargo when $HOME points at an empty, isolated directory (e.g. acceptance
# checks): reuse the invoking user's toolchain (rustup) and crate cache instead
# of downloading them again. Explicit CARGO_HOME / RUSTUP_HOME win.
real_home=$(getent passwd "$(id -un)" 2>/dev/null | cut -d: -f6)
[ -n "$real_home" ] || real_home="$HOME"
export CARGO_HOME="${CARGO_HOME:-$real_home/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$real_home/.rustup}"
exec cargo "$@"
