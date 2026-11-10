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
#
# The OrangeFox ramdisk carries neither OpenSSL nor the Wi-Fi supplicant's
# HIDL interfaces, and the phone's own supplicant and cnss-daemon will not
# start without them. The interfaces are the same ones the OnePlus 8T image
# already carries; libssl is the build that goes with the ramdisk's own
# libcrypto. Neither is copied over a library the ramdisk already has.
set -eu

SKYWARD=${SKYWARD:-/home/bruce/skyward}
WORK=${WORK:-/home/bruce/skyward-work}
OUT=${OUT:-$WORK/out/skyward-op4e5d-$(date +%Y%m%d)-coloros.img}

STOCK=$HOME/ofox_12.1/out/target/product/OP4E5D/recovery.img
UPDATE_ENGINE=$HOME/ofox_12.1/out/target/product/opkona/recovery/root
SUPPORT=$WORK/support/op4e5d

cd "$SKYWARD"

ANDROID_NDK_HOME=${ANDROID_NDK_HOME:-$HOME/android-sdk/ndk/28.2.13676358} \
  cargo ndk -t arm64-v8a --platform 30 build --release -p skyward
cargo build --release -p skyward-pack

target/release/skyward-pack recovery \
  --stock "$STOCK" \
  --third-party \
  --binary target/aarch64-linux-android/release/skyward \
  --update-engine-root "$UPDATE_ENGINE" \
  --library "$SUPPORT/lib64/android.hardware.wifi.supplicant@1.0.so" \
  --library "$SUPPORT/lib64/android.hardware.wifi.supplicant@1.1.so" \
  --library "$SUPPORT/lib64/android.hardware.wifi.supplicant@1.2.so" \
  --library "$SUPPORT/lib64/android.hardware.wifi.supplicant@1.3.so" \
  --library "$SUPPORT/lib64/android.hardware.wifi@1.0.so" \
  --library "$SUPPORT/lib64/libssl.so" \
  --config devices/op4e5d.toml \
  --font resources/fonts/OPPOSans4.0SC-VF.ttf \
  --hero resources/hero/coloros-hero.png \
  --mono resources/fonts/KirkMono-Regular.ttf \
  --mono-license resources/fonts/KirkMono-OFL.txt \
  --diagnostics \
  --out "$OUT"

echo "image: $OUT"
sha256sum "$OUT"
