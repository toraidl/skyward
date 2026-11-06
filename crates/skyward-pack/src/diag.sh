#!/system/bin/sh
# Leaves a snapshot of the boot where a computer can read it after the next
# boot. The stock init mounts the persist partition read-write before any
# recovery starts, and a recovery whose port never comes up cannot be read
# over adb, so this is the only place a failed boot can say what went wrong.
#
# $1 where to write it, $2 how long to wait before writing it again.

out="$1"
delay="$2"

dump() {
    echo "### snapshot $1 at $(date)"
    echo "--- cmdline"
    cat /proc/cmdline
    echo "--- uptime"
    cat /proc/uptime
    echo "--- properties"
    getprop
    echo "--- udc"
    ls -l /sys/class/udc
    for u in /sys/class/udc/*; do
        echo "udc $u"
        cat "$u/state"
        cat "$u/../mode" 2>/dev/null
    done
    echo "--- gadget"
    ls -l /config/usb_gadget/g1
    echo "--- functions"
    ls -l /config/usb_gadget/g1/functions
    echo "--- config b.1"
    ls -l /config/usb_gadget/g1/configs/b.1
    echo "--- usb-ffs"
    ls -l /dev/usb-ffs
    ls -l /dev/usb-ffs/adb
    echo "--- mounts"
    cat /proc/mounts
    echo "--- dmesg"
    dmesg
    echo "--- recovery log"
    cat /tmp/recovery.log
}

dump early >"${out}.early" 2>&1
sync
sleep "$delay"
dump late >"$out" 2>&1
sync
