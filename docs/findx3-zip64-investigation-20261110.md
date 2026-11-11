# Find X3 ZIP64 / sideload investigation (2026-11-10)

## Scope and current result

Device: Find X3 / OP4E5D, USB serial `7f95dee9`, in recovery. Source baseline:
`4888cec`. These observations do not establish OnePlus 8T behavior.

The package's updater supports ZIP64, with a defective extra-field parser.
That defect explains the synthetic package's `bad LFH offset` failure. It
does **not** explain the original OTA's SIGBUS: a read-only probe retaining
that OTA's size and directory layout succeeds both locally and through FUSE.
The earlier SIGBUS occurrence remains unexplained and needs a trace of its
first failed read and the child process's fault location. Later saved logs
show three separate attempts of this OTA returning script status 0 before
failing during USB handback; that is a distinct failure path.

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

The updater hash identifies the parser discussed here. The package has not
undergone a full payload CRC/signature verification in this investigation.

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
| Physical FunctionFS sideload of the probe | not completed; see USB boundary below |
| Original OTA through full installation | still unresolved; earlier log records SIGBUS |

The forwarded case uses the real host `adb sideload`, Skyward's `adb::Connection`
and FUSE server. It bypasses the recovery's FunctionFS endpoint implementation
and gadget transition. It therefore does not validate the physical sideload
USB path. All these cases also bypass the full installation's service teardown.

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

## Diagnostic changes and next repair steps

This commit adds diagnostics without changing transfer or ZIP handling:

- On the first FUSE source fault, record the request offset/size, block index,
  block size, package size and original source error. An mmap page-read error
  can otherwise surface in the updater as SIGBUS.
- Before `conclude` turns a computer-source fault into generic `Disconnected`,
  record both the fault and installer result, including the script exit code.
- Record FunctionFS bind, unbind, enable, disable, suspend and resume events to
  correlate USB transitions with the failed read.

The changes require building and deploying a recovery before they can explain
another physical sideload failure. This investigation has not deployed them.
Validation: `cargo test -p skyward-install --locked` passes all 68 tests, and
`cargo ndk -t arm64-v8a --platform 30 check --release -p skyward-install --locked`
passes. `git diff --check` passes. A rustfmt check was unavailable because the
installed stable toolchain has no rustfmt component.

Proceed in this order:

1. Run the original-layout read-only probe through the recovery-owned sideload
   path. Avoid launching a gadget-switching diagnostic under adbd. Preserve
   existing logs before retrying; do not use the real writing script yet.
2. If it fails, collect the new FUSE fault and USB transition lines together
   with the script log. Capture SIGBUS `si_code`, fault address, PC and memory
   mappings with a scoped native trace. Relate the fault address to the package
   mapping's file offset, and compare those bytes with the host archive.
3. A USB disable/shutdown preceding a failed block points to gadget/FunctionFS
   ownership or teardown. A protocol error points to the transfer implementation.
   A fault with correct file bytes and no source error points back to updater
   parsing/mapping. Each requires a different fix.
4. For the confirmed extra-field defect, prefer fixing/rebuilding the updater's
   ZIP64 parser with bounded field traversal while retaining its Oplus-specific
   functions. Reordering fields can demonstrate or work around the defect in a
   newly built package, but changes signed bytes and needs package re-signing;
   it is not a transparent repair of an existing signed OTA.
5. Validate the physical read-only case before a real OTA. Full OTA acceptance
   still requires successful writing, cleanup and boot; none is proved here.

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

The saved Skyward recovery log also contains three attempts offering the full
8,931,218,810-byte OTA. Each ends with `install: Ran { wipe_cache: false }`.
In the matching source, `Outcome::Ran` is emitted only when the update script
returns code 0. All three then fail to restore the resting USB gadget: one
reports `name the MTP function: File exists`, and two report
`give the port its configuration: No such device`. This does not prove the
resulting partitions boot correctly, but it moves the observed failure after
the updater's zero exit and into USB handback. Do not attribute these runs to
the synthetic ZIP64 offset defect.

OrangeFox's install path also unmounts `/vendor` before running the package
updater by default. Skyward's earlier `vendor stayed busy` attempts failed
before the updater and are a separate, pre-run failure. The source follow-up
waits for adbd's FunctionFS readiness before reordering MTP/ADB configfs links
and retries bounded `EEXIST`, `EBUSY`, and `ENODEV` races. It is host-tested and
built, but not yet deployed on the phone.
