#!/usr/bin/env bash
# Proves the product builds without dev/: copies the repo minus dev/ (and the members glob) to a temp
# folder, then builds, lints and compiles the tests there. The working tree is left untouched.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
tar -C "$root" --exclude=./dev --exclude=./target --exclude=./.git -cf - . | tar -C "$tmp" -xf -
sed -i 's|, "dev/\*"||' "$tmp/Cargo.toml"
! grep -q 'dev/' "$tmp/Cargo.toml"
cd "$tmp"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"
cargo build --workspace --release
cargo clippy --workspace
cargo test --workspace --no-run
echo "OK: builds without dev/"
