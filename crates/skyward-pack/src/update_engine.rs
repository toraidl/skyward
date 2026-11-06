//! Keep a replacement OTA engine and its matching runtime separate from
//! the stock runtime used by Keymaster and Gatekeeper.

use anyhow::{ensure, Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{ramdisk::Ramdisk, support_elf};

const HOME: &str = "system/skyward-update-engine";
const PROGRAM: &str = "system/bin/update_engine_sideload";

pub fn bundle(rd: &mut Ramdisk, root: &Path) -> Result<()> {
    let engine = root.join(PROGRAM);
    let linker = root.join("system/bin/linker64");
    let mut pending = vec![engine.clone(), linker.clone()];
    let mut seen = BTreeSet::new();
    while let Some(file) = pending.pop() {
        if !seen.insert(file.clone()) {
            continue;
        }
        let data = support_elf(&file)?;
        for name in needed(&file)? {
            pending.push(root.join("system/lib64").join(name));
        }
        let destination = if file == engine {
            format!("{HOME}/update_engine_sideload")
        } else if file == linker {
            format!("{HOME}/linker64")
        } else {
            format!("{HOME}/lib64/{}", file.file_name().context("library has no name")?.to_str().context("non UTF-8 library name")?)
        };
        rd.add(&destination, if file == engine || file == linker { 0o755 } else { 0o644 }, data)?;
        println!("OTA runtime: {} -> /{destination}", file.display());
    }
    // Run the matching linker directly: replacing only the engine can leave
    // unresolved libcrypto/libziparchive symbols on older stock recoveries.
    // No LD_PRELOAD or inherited search path may inject another runtime.
    let wrapper = format!("#!/system/bin/sh\nunset LD_PRELOAD\nexport LD_LIBRARY_PATH=/{HOME}/lib64\nexec /{HOME}/linker64 /{HOME}/update_engine_sideload \"$@\"\n").into_bytes();
    if rd.find(PROGRAM).is_some() {
        rd.replace(PROGRAM, wrapper)?;
    } else {
        rd.add(PROGRAM, 0o755, wrapper)?;
    }
    Ok(())
}

fn needed(file: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("readelf").args(["--dynamic", "--wide"]).arg(file).output().context("run readelf for the OTA runtime")?;
    ensure!(output.status.success(), "readelf {}: {}", file.display(), String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).context("readelf output is not UTF-8")?;
    let mut names = Vec::new();
    for line in text.lines().filter(|l| l.contains("(NEEDED)")) {
        let name = line.split_once('[').and_then(|(_, s)| s.split_once(']').map(|(n, _)| n)).context("readelf NEEDED entry has no library name")?;
        ensure!(!name.is_empty() && !name.contains('/') && name != "." && name != "..", "invalid dependency {name}");
        names.push(PathBuf::from(name));
    }
    Ok(names)
}
