#!/system/bin/sh
# Persistent, bounded boot snapshots, independent of /data and Skyward's UI.
# $1 fallback directory on the already mounted persist filesystem; $2 interval.
# On OnePlus 8T, op2 is the ext4 log filesystem. Never write raw reserve slots.

umask 077
fallback="$1"
delay="${2:-3}"
case "$delay" in ''|*[!0-9]*|0) delay=3 ;; esac
exec >>/tmp/skyward-diag-service.log 2>&1

mounted() {
    awk -v at="$1" '$2 == at { found=1 } END { exit !found }' /proc/mounts
}

choose_directory() {
    logmount=/mnt/vendor/oplusreserve
    # The stock ramdisk has the blkid applet but no standalone symlink and
    # its older toybox does not implement the newer '-o value' option.
    if /system/bin/toybox blkid /dev/block/by-name/op2 2>/dev/null | grep -q 'TYPE="ext4"'; then
        mkdir -p "$logmount"
        if ! mounted "$logmount"; then
            mount -t ext4 -o rw,nosuid,nodev,noatime /dev/block/by-name/op2 "$logmount"
        fi
        if mounted "$logmount" && mkdir -p "$logmount/skyward"; then
            root="$logmount/skyward"
            return 0
        fi
    fi
    # Do not mistake an unmounted directory in the ramdisk for durable storage.
    if mounted "${fallback%/*}" && mkdir -p "$fallback"; then
        root="$fallback"
        return 0
    fi
    return 1
}

attempt=0
until choose_directory; do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 30 ]; then
        echo '<3>skyward-diag: no writable persistent log filesystem' >/dev/kmsg
        exit 1
    fi
    sleep 1
done
chmod 0700 "$root"

# Two boots only, with a fixed size bound for each file. A restart of this
# service in the same boot keeps the previous boot intact.
boot_id=$(cat /proc/sys/kernel/random/boot_id)
if [ "$(cat "$root/current/boot-id" 2>/dev/null)" != "$boot_id" ]; then
    rm -rf "$root/previous"
    if [ -d "$root/current" ]; then
        mv "$root/current" "$root/previous" || exit 1
    fi
fi
mkdir -p "$root/current" || exit 1
out="$root/current"
printf '%s\n' "$boot_id" >"$out/boot-id"
printf '%s\n' "$out" >/tmp/skyward-logdir
echo "<6>skyward-diag: persistent snapshots in $out" >/dev/kmsg

{
    echo 'Skyward diagnostic format 2'
    date
    uname -a
    sha256sum /system/bin/recovery
    cat /proc/cmdline
} >"$out/build.txt.tmp" 2>&1
mv "$out/build.txt.tmp" "$out/build.txt"
sync

status() {
    date
    cat /proc/uptime
    echo '--- properties'
    # Query only boot, crypto and USB properties. Enumerating every Android
    # property maps hundreds of unrelated contexts and flooded the recovery
    # audit log with getprop denials on the OnePlus stock policy.
    for key in ro.boot.serialno ro.product.model ro.boot.slot_suffix \
        ro.build.version.release ro.build.version.security_patch ro.vendor.build.security_patch \
        hwservicemanager.ready vendor.sys.listeners.registered vendor.gatekeeper.disable_spu \
        logd.ready sys.usb.config sys.usb.state sys.usb.controller sys.usb.configfs \
        sys.usb.ffs.ready sys.usb.ffs.mtp.ready \
        init.svc.recovery init.svc.logd init.svc.adbd init.svc.fastbootd \
        init.svc.gatekeeperd init.svc.hal_cryptoeng_oplus \
        init.svc.vendor.keymaster-4-1 init.svc.vendor.gatekeeper-1-0 \
        init.svc.vendor.keymasterd init.svc.vendor.qseecomd; do
        printf '[%s]: [%s]\n' "$key" "$(getprop "$key")"
    done
    echo '--- processes'
    ps -A -o USER,PID,PPID,NAME,ARGS
    echo '--- mounts'
    cat /proc/mounts
    echo '--- usb'
    ls -l /config/usb_gadget/g1/configs/b.1 /dev/usb-ffs/adb
    for u in /sys/class/udc/*; do
        echo "$u"
        cat "$u/state" "$u/current_speed" 2>/dev/null
    done
    echo '--- diagnostic service'
    tail -c 8192 /tmp/skyward-diag-service.log
}

while :; do
    # Same-filesystem rename leaves the preceding complete snapshot readable
    # if power is lost while the next one is being written. No unbounded files.
    status 2>&1 | tail -c 131072 >"$out/status.txt.tmp"
    mv "$out/status.txt.tmp" "$out/status.txt"
    dmesg 2>&1 | tail -c 524288 >"$out/kernel.log.tmp"
    mv "$out/kernel.log.tmp" "$out/kernel.log"
    cat /tmp/recovery.log 2>&1 | tail -c 262144 >"$out/recovery.log.tmp"
    mv "$out/recovery.log.tmp" "$out/recovery.log"
    if [ -f /tmp/update_engine.log ]; then
        tail -c 524288 /tmp/update_engine.log >"$out/update_engine.log.tmp"
        mv "$out/update_engine.log.tmp" "$out/update_engine.log"
    fi
    for stream in stdout stderr; do
        if [ -f "/tmp/skyward-wlan/supplicant.$stream" ]; then
            tail -c 65536 "/tmp/skyward-wlan/supplicant.$stream" >"$out/supplicant.$stream.tmp"
            mv "$out/supplicant.$stream.tmp" "$out/supplicant.$stream"
        fi
    done
    timeout 3 logcat -b all -d -t 1600 2>&1 | tail -c 524288 >"$out/logcat.log.tmp"
    mv "$out/logcat.log.tmp" "$out/logcat.log"
    date >"$out/last-sync.txt.tmp"
    mv "$out/last-sync.txt.tmp" "$out/last-sync.txt"
    sync
    sleep "$delay"
done
