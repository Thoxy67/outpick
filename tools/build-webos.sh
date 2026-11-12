#!/bin/sh
# Cross-builds the TV binary (armv7 soft-float ABI, webOS NDK sysroot), after
# Luma's tools/build-webos.sh. Run tools/setup-webos-sdk.mjs once first.
set -eu
ROOT=$(dirname "$(dirname "$(realpath "$0")")")
WEBOS_NDK=${WEBOS_NDK:-$ROOT/.build/toolchain/arm-webos-linux-gnueabi_sdk-buildroot}
CC="$WEBOS_NDK/bin/arm-webos-linux-gnueabi-gcc"
[ -x "$CC" ] || { echo "Missing webOS compiler: $CC (run: node tools/setup-webos-sdk.mjs)" >&2; exit 1; }
SYSROOT=$("$CC" -print-sysroot)
[ -f "$SYSROOT/usr/include/stdint.h" ] || SYSROOT="$WEBOS_NDK/arm-webos-linux-gnueabi/sysroot"
node "$ROOT/tools/webos-runtime.mjs"
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABI_LINKER="$CC"
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABI_RUSTFLAGS="-C target-cpu=cortex-a9 -C target-feature=-neon,-soft-float -C link-arg=--sysroot=$SYSROOT -C link-arg=-Wl,-rpath,\$ORIGIN/lib"
export CC_armv7_unknown_linux_gnueabi="$CC"
export AR_armv7_unknown_linux_gnueabi="$WEBOS_NDK/bin/arm-webos-linux-gnueabi-ar"
export CFLAGS_armv7_unknown_linux_gnueabi="--sysroot=$SYSROOT"
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_PATH=
export PKG_CONFIG_SYSROOT_DIR=/
export PKG_CONFIG_LIBDIR="$ROOT/.build/sdl-webos/pkgconfig"
exec cargo build --manifest-path "$ROOT/Cargo.toml" --target armv7-unknown-linux-gnueabi --no-default-features --features webos --release "$@"
