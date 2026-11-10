#!/usr/bin/env bash
# Builds the Linux binary against glibc 2.31, so it runs on Ubuntu 20.04 / Debian 11 and newer
# (a plain `cargo build` on a recent distro needs that distro's glibc: issue #9).
# matlivre links no GUI library (X11, Wayland, OpenGL and the file dialogs load at run time), so
# only libc, libm, libgcc_s, libpthread and libdl are needed.
# One-time setup: cargo install cargo-zigbuild --locked, and Zig (https://ziglang.org/download)
# on the PATH. Output: $CARGO_TARGET_DIR (default target)/x86_64-unknown-linux-gnu/release/.
set -euo pipefail
cd "$(dirname "$0")/../.."
rustup target add x86_64-unknown-linux-gnu >/dev/null
cargo zigbuild --release -p livre-app --bin matlivre --target x86_64-unknown-linux-gnu.2.31
bin=${CARGO_TARGET_DIR:-target}/x86_64-unknown-linux-gnu/release/matlivre
newest=$(objdump -T "$bin" | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -1)
echo "$bin needs at most $newest"
[ "$(printf '%s\nGLIBC_2.31\n' "$newest" | sort -V | tail -1)" = GLIBC_2.31 ] || { echo "error: needs glibc newer than 2.31" >&2; exit 1; }
