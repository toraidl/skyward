#!/bin/sh
# Rebuild the OPPO Find X3 (OP4E5D / PEDM00) internal ColorOS recovery
# image.
#
# The phone's stock recovery is not part of its ColorOS update packages: it
# is the boot image with a patch applied by vendor's install-recovery
# script, so no recovery partition image is shipped. The OrangeFox recovery
# built in ~/ofox_12.1 already carries the phone's init, adbd, SELinux
# policy and the secure services this build needs, so it is used as the
# base with --third-party and only its recovery program is replaced.
#
# The recovery's own logd in that ramdisk is complete, so no --logd is
# passed. update_engine_sideload comes from the OnePlus 8T (opkona) tree of
# the same source, where it was patched and proven to apply packages.
set -eu

SKYWARD=${SKYWARD:-/home/bruce/skyward}
WORK=${WORK:-/home/bruce/skyward-work}
OUT=${OUT:-$WORK/out/skyward-op4e5d-$(date +%Y%m%d)-coloros.img}

STOCK=$HOME/ofox_12.1/out/target/product/OP4E5D/recovery.img
UPDATE_ENGINE=$HOME/ofox_12.1/out/target/product/opkona/recovery/root

cd "$SKYWARD"

ANDROID_NDK_HOME=${ANDROID_NDK_HOME:-$HOME/android-sdk/ndk/28.2.13676358} \
  cargo ndk -t arm64-v8a --platform 30 build --release -p skyward
cargo build --release -p skyward-pack

target/release/skyward-pack recovery \
  --stock "$STOCK" \
  --third-party \
  --binary target/aarch64-linux-android/release/skyward \
  --update-engine-root "$UPDATE_ENGINE" \
  --config devices/op4e5d.toml \
  --font resources/fonts/OPPOSans4.0SC-VF.ttf \
  --hero resources/hero/coloros-hero.png \
  --mono resources/fonts/KirkMono-Regular.ttf \
  --mono-license resources/fonts/KirkMono-OFL.txt \
  --diagnostics \
  --out "$OUT"

echo "image: $OUT"
sha256sum "$OUT"
