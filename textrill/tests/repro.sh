#!/bin/sh
# Build a crate twice in clean target directories and require the release
# binary to be byte-identical. The compiler is pinned by rust-toolchain.toml.
#
# Usage: repro.sh MANIFEST BINARY [TARGET]
set -eu

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
    echo "usage: $0 MANIFEST BINARY [TARGET]" >&2
    exit 2
fi

manifest=$1
binary=$2
target=${3:-}

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"

if [ -n "$target" ] && command -v rustup >/dev/null 2>&1; then
    rustup target list --installed | grep -qx "$target" || {
        echo "repro: missing target $target; run: rustup target add $target" >&2
        exit 1
    }
fi

epoch=$(git log -1 --format=%ct)
cargo_home=${CARGO_HOME:-${HOME:-}/.cargo}
sysroot=$(rustc --print sysroot)
work=$(mktemp -d "${TMPDIR:-/tmp}/textrill-repro.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM

for n in 1 2; do
    target_dir="$work/target-$n"
    flags="--remap-path-prefix=$root=/textrill --remap-path-prefix=$target_dir=/target"
    flags="$flags --remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$sysroot=/rustc"
    if [ -n "$target" ]; then
        CARGO_TARGET_DIR="$target_dir" SOURCE_DATE_EPOCH="$epoch" \
            CARGO_INCREMENTAL=0 RUSTFLAGS="$flags" \
            cargo build --release --locked --manifest-path "$manifest" --target "$target"
        artifact="$target_dir/$target/release/$binary"
    else
        CARGO_TARGET_DIR="$target_dir" SOURCE_DATE_EPOCH="$epoch" \
            CARGO_INCREMENTAL=0 RUSTFLAGS="$flags" \
            cargo build --release --locked --manifest-path "$manifest"
        artifact="$target_dir/release/$binary"
    fi
    cp "$artifact" "$work/$binary-$n"
done

sha256sum "$work/$binary-1" "$work/$binary-2"
if cmp -s "$work/$binary-1" "$work/$binary-2"; then
    echo "repro: $binary is byte-identical across two clean builds"
else
    echo "repro: $binary differs across two clean builds" >&2
    exit 1
fi
