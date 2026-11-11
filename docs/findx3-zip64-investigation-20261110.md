# Find X3 ZIP64 / sideload investigation (2026-11-10)

## Scope and current result

Device: Find X3 / OP4E5D, USB serial `7f95dee9`, in recovery. Source baseline:
`4888cec`. These observations do not establish OnePlus 8T behavior.

The proprietary updater contains a ZIP64 extra-field offset bug demonstrated
with a synthetic archive. The original OTA orders its ZIP64 offset field first,
so that specific bug does not explain the original-layout probe results. REA
also found that `my_stock.new.dat` has a ZIP64 size sentinel which is passed to
a 32-bit inflater check. The block-image worker still receives the inflater's
output chunks, and saved full-OTA attempts returned script status 0; REA does
not establish that the size mismatch aborts this package's update path.

The observed Skyward “install failure” in saved full-OTA runs was logged after
the updater returned status 0, during USB handback. Live configfs readback
later showed that init had restored a valid OP4E5D gadget (`mtp.gs0` plus
`ffs.adb`); previous Skyward code incorrectly required `ffs.mtp` and reported
a false failure. That handback bug is fixed and its no-op sideload path is
verified on device. A full OTA has not been rerun, so partition contents and
post-OTA boot remain unverified.

During the original ZIP/updater experiments, no system partition was written
or recovery image flashed. The later OrangeFox follow-up below did flash the
recovery partition after backing it up. Probe files and native diagnostic
executables were placed in `/tmp`; the source OTA was not modified. A sparse
probe is a diagnostic artifact, not an installable replacement OTA or an
integrity-verified package.

## Artifact identity

- Original package:
  `ota_full-PEDM00_17.0.1.203-PYB110-11101443-CN-2026-11-01-d5cd9b1ff8.zip`
- Original size: `8931218810` bytes; 78 directory entries.
- Original central-directory offset: `8931211245`.
- Extracted update-binary: 2324120 bytes, arm64.
- update-binary SHA-256:
  `6f1a8079a8c9b0b65b99f5d78298828d9329aeddd923f03ea8d5ec4d20b312ee`.

The updater hash identifies the parser discussed here. Host `unzip -tq` read
the complete original OTA and reported no compressed-data/CRC errors. This
checks ZIP member integrity; it does not verify the package signature or
successful application to device partitions.

## Confirmed extra-field parser defect

The synthetic 5371036732-byte ZIP contains a 5 GiB dummy entry and a short
script with device-property assertions and `ui_print`, without partition writes.
Its metadata entry has real local-header offset `5371034309`.

The central-directory extra fields for that entry are ordered as:

1. `0x5455`: timestamp.
2. `0x7875`: Unix owner IDs.
3. `0x0001`: ZIP64 local-header offset.

Running the original updater against the local sparse file reports:

```text
Zip: bad LFH offset 826539633824508675 at entry 7
exit=1
```

The erroneous integer is `0x0b78756af2e42b03`, exactly the little-endian
interpretation of eight bytes starting four bytes into that extra-field area:
timestamp data followed by bytes from the next field. It is not the true offset.

Moving `0x0001` ahead of the other fields, without changing file size, offsets,
payload, or script, makes the same updater return `exit=0` and print the target.

REA/Ghidra inspection of this exact binary confirms that `OpenArchiveInternal`
at `0x328af0`, on encountering a `0xffffffff` local-header offset, loads eight
bytes at `central_record + filename_length + 0x32`. It does not walk the extra
fields by their IDs. The same function's custom ZIP64 branch requires a memory
mapping and rejects more than 255 entries. Those are static observations of
this binary; larger-entry-count behavior was not exercised on the phone.

Format references:

- [PKWARE APPNOTE, section 4.5.3](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT)
- [AOSP libziparchive ZIP64 parsing](https://android.googlesource.com/platform/system/core/+/1b1ec8f36a39874fb0420d9dffef7ed76812c9ec/libziparchive/zip_archive.cc)

The original OTA's entries beyond 4 GiB already have `0x0001` first, with an
8-byte offset value. Reordering those entries would not fix the reported crash.

## Read-only probe matrix

An additional sparse probe retains the original OTA's logical size, all 78
central-directory entries, ZIP64 offsets and footer. Only the beginning of
the archive and its directory/footer are materialized. The updater script is
replaced, with its header sizes and CRC updated, by exactly:

```text
ui_print("SKYWARD ZIP64 READ-ONLY PROBE OK");
```

Most payload contents are holes. Success means the archive can be opened and
this script executed, not that the payload can be installed.

| Path | Observed result |
| --- | --- |
| 5.37 GB local sparse ZIP, timestamp field first | `bad LFH offset`, exit 1 |
| Same ZIP, ZIP64 field first | target printed, exit 0 |
| 8.93 GB original-layout probe, local file | probe printed, exit 0 |
| Same probe through current Skyward FUSE, local source, cold cache | probe printed, exit 0; no source fault |
| FUSE reads around 2 GiB, 4 GiB and EOF | `pread` and private `mmap` equal local bytes; child exits 0 |
| Host `adb sideload` over a forwarded diagnostic ADB stream, then the same Connection/FUSE/updater | probe printed, updater exits 0; `finish=Ok(())` |
| Physical FunctionFS sideload of the original-layout no-op probe | script returned 0 and USB handback succeeded after the fix; host reported `Total xfer: 0.00x`, so this is not full-payload validation |
| Full host `unzip -tq` over the original OTA | all compressed members passed ZIP CRC checks |
| Original OTA through full installation | not rerun after the USB fix; saved pre-fix runs returned script status 0, but partition contents and boot were not verified |

The forwarded case uses the real host `adb sideload`, Skyward's `adb::Connection`
and FUSE server. It bypasses the recovery's FunctionFS endpoint implementation
and gadget transition. The later physical no-op case validates sideload entry
and handback, but its zero host transfer result and no-op script do not validate
reading or installing the full OTA payload.

## USB boundary

A diagnostic started from an ADB shell attempted to take USB over using the
same FunctionFS module. USB disappeared before it accepted the package and
its log remained empty. Starting a new session with `setsid` does not move a
process out of adbd's Android cgroup; a later shell reports membership in
`/uid_0/pid_1448`. Termination along with adbd is a plausible explanation, not
a captured process-exit fact. Do not repeat this launch pattern.

The user restored connectivity. The same recovery boot remained running;
USB ADB was verified again as serial `7f95dee9`, with `sys.usb.state=mtp,adb`.
The temporary forwarded ADB connection and port mapping were removed.

## Diagnostic changes and remaining scope

This commit adds diagnostics without changing transfer or ZIP handling:

- On the first FUSE source fault, record the request offset/size, block index,
  block size, package size and original source error. An mmap page-read error
  can otherwise surface in the updater as SIGBUS.
- Before `conclude` turns a computer-source fault into generic `Disconnected`,
  record both the fault and installer result, including the script exit code.
- Record FunctionFS bind, unbind, enable, disable, suspend and resume events to
  correlate USB transitions with the failed read.

The USB source change has since been built and deployed by temporary
`fastboot boot`; the physical no-op sideload confirmed USB handback. The source
removes the custom configfs link rewrite and waits for init's adbd, FunctionFS,
and USB-state properties. `cargo test --locked -p skyward-sys` passes all 35
tests. `cargo fmt --all -- --check` is unavailable because rustfmt is not
installed for the configured toolchain.

The ZIP64 defects are properties of the OTA's proprietary `update-binary`,
which is supplied by the package and is not built from this repository.
Replacing or patching it would require preserving Oplus-specific updater
behavior and validating a modified package. The original OTA uses
`block_image_update` for `my_stock.new.dat`, and saved runs report script status
0; available evidence does not justify changing that binary as the fix for the
observed Skyward failure.

Full OTA acceptance still needs a separate run of the real package followed
by partition and boot verification. Its updater script writes boot, vbmeta,
firmware, and dynamic partitions, so live validation here used a read-only
no-op package only.

Local diagnostic artifacts from this session live under
`/home/bruce/skyward-work/zip64-diag/` (`local-comparison.log`, `tunnel.log`,
`usb-diag.log`, sparse probe, Cargo project). They are not recovery build inputs.

## Follow-up (2026-11-11)

The user confirmed that this exact OTA completed installation in OrangeFox.
The OrangeFox `recovery.img` in `~/coloros_port/devices/OP4E5D/` was an older
TWRP image. The latest OP4E5D OrangeFox build is
`~/ofox_12.1/out/target/product/OP4E5D/recovery.img`, product `ofox_OP4E5D`,
dated 2026-10-15. It was written to the 128 MiB `recovery` partition after
backing up the prior bytes; the post-write readback matched SHA-256
`af90570c0cd22bcb5d51d5c96e881f68b6379c4f79bee31124a96e7baeed984b`.
The phone booted the image as `fox_OP4E5D` (OrangeFox R12.0).

A 305-byte unsigned test ZIP containing only a no-op update-binary was sent
with `adb sideload` to serial `7f95dee9`. The host reported `Total xfer: 1.00x`.
This validates USB sideload transport on this OrangeFox build without writing
phone partitions. The captured OrangeFox log at
`~/skyward-work/out/op4e5d-orangefox-sideload-noop.log` records the 305-byte
input, the `Skyward OrangeFox sideload transport check` marker, `Process ended
with no errors`, and `Install took 1 second(s)`; its SHA-256 is
`55c670c11634a824cd61456482a8b89154c82abc61160062e02fa0ea1a0dc78e`. This
proves transport and updater launch for the no-op package, not completion of
the 8.93 GB OTA through OrangeFox. The user reports that the full OTA completed
there, but that installation log was not captured in this session.

The saved Skyward recovery log contains three attempts offering the full
8,931,218,810-byte OTA. Each ends with `install: Ran { wipe_cache: false }`;
the matching source emits `Outcome::Ran` only when the update script returns
code 0. The old post-script USB code then reported either `name the MTP
function: File exists` or `give the port its configuration: No such device`.
Subsequent configfs readback established that OP4E5D uses `mtp.gs0`, not the
hardcoded `ffs.mtp` link the code expected. These messages identify a faulty
handback path, not a proven OTA partition failure. They do not prove partition
contents or a successful boot.

OrangeFox's install path also unmounts `/vendor` before running the package
updater by default. Skyward's earlier `vendor stayed busy` attempts failed
before the updater and are a separate, pre-run failure. The first source
follow-up attempted to reorder MTP/ADB configfs links. Live readback showed
that approach was wrong for OP4E5D; commit `9dd969f` removes it and leaves link
ownership with init.

## REA trace of the 5 GiB ZIP64 member (2026-11-11)

The original `my_stock.new.dat` entry is DEFLATE, with uncompressed size
`5,011,189,760` bytes and compressed size `4,173,225,454` bytes. Its central
directory's 32-bit uncompressed-size field is `0xffffffff`; the ZIP64 extra
field carries the real 64-bit size. The v4 `my_stock.transfer.list` declares
`1,223,435` 4 KiB blocks and contains 1,195 `new` ranges. Those blocks account
for exactly `5,011,189,760` bytes; `my_stock.patch.dat` is empty. Applying this
partition therefore needs the complete decompressed member.

REA traced the updater binary with SHA-256
`6f1a8079a8c9b0b65b99f5d78298828d9329aeddd923f03ea8d5ec4d20b312ee`:

1. `PerformBlockImageUpdate` (`0x2a9848`) starts `unzip_new_data`
   (`0x2b4b98`) as a worker.
2. `unzip_new_data` passes its `ZipEntry` to `ProcessZipEntryContents`
   (`0x32a60c`), which calls `ExtractToWriter` (`0x32a09c`).
3. `ExtractToWriter` passes 32-bit compressed and uncompressed sizes from the
   `ZipEntry` to `Inflate` (`0x329d08`). For this entry, the uncompressed size
   supplied by `FindEntry` is the ZIP64 sentinel `0xffffffff`.
4. `Inflate` continues feeding output to its writer until the DEFLATE stream
   ends, then checks the accumulated output size against that 32-bit value and
   returns a size-mismatch error. `ProcessZipEntryContents` discards
   `ExtractToWriter`'s result, and `unzip_new_data` also discards that helper's
   result. The `receive_new_data` callback writes each produced chunk to the
   block-image range writer.

This explains how the same embedded updater can report a post-inflate size
mismatch while still feeding the full data stream to `block_image_update`.
The 32-bit size limitation affects extraction paths that propagate this
error, but REA does not show it aborting the block-image worker path used for
`my_stock.new.dat`. This is consistent with the user's report that OrangeFox
completed the same OTA and with Skyward logs that show the script returned
status 0. Those facts do not independently verify the resulting partition
contents or a successful boot.

The earlier `physical-test-3` only read the member's compressed byte range to
`/dev/null`; it did not inflate the 5 GiB output. The OrangeFox log captured in
this session is also only for a 305-byte no-op ZIP. No full OTA was started in
this follow-up. The ZIP64 size mismatch is therefore a real parser limitation,
but it is not supported as the cause of the reported Skyward install failure.

At the time of this REA trace, the USB handback cause had not been identified.
The following section records the later configfs evidence, code correction,
and live no-op retest.

REA evidence export: `/home/bruce/skyward-work/zip64-diag/rea-evidence-20261111-final.json`
(53 records, 12,202,242 bytes). Snapshot:
`/home/bruce/skyward-work/zip64-diag/rea-snapshot-20261111-final.json`.

## USB handback correction and rebuilt image (2026-11-11)

The post-probe recovery log ended with a Skyward MTP/ADB handback timeout,
but live configfs readback showed that the gadget had in fact returned to
`mtp,adb`: `configs/b.1/f1` pointed to `functions/mtp.gs0`, `f2` pointed to
`functions/ffs.adb`, `init.svc.adbd=running`, and `sys.usb.ffs.ready=1`.
OP4E5D's init uses `mtp.gs0`; the prior Skyward implementation incorrectly
required `f1` to point to `ffs.mtp`, then rewrote init-owned configfs links and
reported failure when they did not match. This is a validator/ownership bug,
not evidence that the device failed to restore USB. The same no-op sideload
log shows the 8,931,218,810-byte ZIP was offered, its install script ran, and
the USB gadget was unbound and handed back. Because the ZIP script did no
payload extraction or partition write, this does not verify a full OTA.

`crates/skyward-sys/src/usb.rs` now lets init select the MTP function and
waits for adbd running, FunctionFS readiness, and `sys.usb.state=mtp,adb`;
it no longer edits configfs links itself. `cargo test --locked -p skyward-sys`
passes all 35 tests. The OP4E5D image rebuilt successfully and was checked as
an Android boot image; the packer listing includes the recovery init, adbd,
and update_engine_sideload runtime. Image:
`/home/bruce/skyward-work/out/skyward-op4e5d-20261111-usb-mtp-handback.img`,
SHA-256 `1c55263448cc0192daee5f884ec19cfc2bf402e362eab29cb1b27e1fa9376f7f`.
After verifying fastboot serial `7f95dee9`, product `kona`, and
`is-userspace: no`, the image was started with `fastboot boot` (no partition
write). The runtime identified as `OP4E5D 0.1.2 recovery up`, initially with
`mtp,adb`, adbd running, and FunctionFS ready.

The same no-op ZIP was then sideloaded. Host `adb sideload` returned
`Total xfer: 0.00x`; the recovery log records sideload USB bound/enabled,
the 8,931,218,810-byte ZIP offer and install-script detection, followed by
sideload USB disabled/unbound and `install: Ran { wipe_cache: false }`.
Afterward serial `7f95dee9` immediately enumerated as recovery over USB;
`sys.usb.config` and `sys.usb.state` were `mtp,adb`, `init.svc.adbd` was
`running`, `sys.usb.ffs.ready` was `1`, and configfs links remained
`mtp.gs0` plus `ffs.adb`. No MTP/ADB timeout occurred. This verifies the
USB-mode handback path with the read-only no-op package. The host's zero
transfer result and no-op updater mean this does not validate transfer,
inflation, or installation of the full 8.93 GB OTA; no recovery partition
write or full OTA was performed.

Captured runtime log:
`/home/bruce/skyward-work/out/op4e5d-skyward-usb-mtp-handback-runtime.log`,
SHA-256 `fa2b3c47f4ae169194ed9ef57443355302a55b9db153b4813a9184f992d3ad0a`.
The live USB handback fix is verified for this no-op sideload flow; a full
OTA run remains unverified.

`cargo fmt --all -- --check` could not run because rustfmt is not installed
for the configured toolchain; no toolchain components were installed.
