//! The functions the USB port offers, which init sets up as
//! `sys.usb.config` asks: adb while the recovery waits, with files over MTP
//! beside it while file transfer is on.

use crate::{prop, set_prop, wait_prop};
use anyhow::{ensure, Context, Result};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static MTP: AtomicBool = AtomicBool::new(false);

/// Where init keeps the gadget and the configuration a port offers.
const GADGET: &str = "/config/usb_gadget/g1";
const CONFIG: &str = "/config/usb_gadget/g1/configs/b.1";

/// What the port offers while nothing else has it.
pub fn resting() -> &'static str {
    if MTP.load(Ordering::Relaxed) {
        "mtp,adb"
    } else {
        "adb"
    }
}

/// Whether MTP goes beside adb while the port rests: from when its function
/// has its descriptors until it goes.
pub fn set_mtp(on: bool) {
    MTP.store(on, Ordering::Relaxed);
}

/// Ask init for `config` and wait until it is in place. Going to "none"
/// also waits for adbd to be gone, so its function is free.
pub fn set(config: &str, timeout: Duration) -> Result<()> {
    set_prop("sys.usb.config", config)?;
    wait_prop("sys.usb.state", config, timeout)?;
    if config == "none" {
        let start = Instant::now();
        while !matches!(prop("init.svc.adbd").as_deref(), None | Some("stopped")) {
            ensure!(start.elapsed() < timeout, "adbd still runs {} ms after the port went to none", timeout.as_millis());
            thread::sleep(Duration::from_millis(10));
        }
    }
    if config == "mtp,adb" {
        name_mtp(timeout)?;
    }
    Ok(())
}

/// Name the MTP function in the port's first link, beside adb in the
/// second.
///
/// The recovery's own init rules put adb in the first link as soon as
/// adbd is ready, and nothing in init can name it otherwise: configfs
/// refuses to replace a link the controller still holds, and init only
/// replaces one by unlinking it first, which it does not do. Unbind, clear
/// both links, name them again, bind, and read back that the first one is
/// the MTP function; adbd re-reading its function around the rebind can
/// leave adb there again, so this tries until the port says what was asked.
fn name_mtp(timeout: Duration) -> Result<()> {
    let udc = Path::new(GADGET).join("UDC");
    let controller = prop("sys.usb.controller").context("sys.usb.controller names no port")?;
    let (first, second) = (Path::new(CONFIG).join("f1"), Path::new(CONFIG).join("f2"));
    let start = Instant::now();
    loop {
        fs::write(&udc, "").context("take the port down")?;
        // init answers a port that went down with its own links again, so
        // let it, then take both off before naming this recovery's.
        thread::sleep(Duration::from_millis(50));
        let _ = fs::remove_file(&first);
        let _ = fs::remove_file(&second);
        fs::write(Path::new(GADGET).join("idProduct"), "0x4EE2").context("name the port's product")?;
        fs::write(Path::new(CONFIG).join("strings/0x409/configuration"), "mtp_adb").context("name the port's configuration")?;
        symlink(format!("{GADGET}/functions/ffs.mtp"), &first).context("name the MTP function")?;
        symlink(format!("{GADGET}/functions/ffs.adb"), &second).context("name the adb function")?;
        fs::write(&udc, &controller).context("give the port its configuration")?;
        thread::sleep(Duration::from_millis(50));
        let named = fs::read_link(&first).is_ok_and(|to| to.ends_with("functions/ffs.mtp"));
        if named {
            return Ok(());
        }
        ensure!(start.elapsed() < timeout, "the port kept adb in its first link for {} ms", timeout.as_millis());
    }
}

/// Hand the port to `config` through "none", which stops the programs
/// serving it and takes every function off; init then sets the port up
/// once the programs behind the new configuration are ready.
pub fn switch(config: &str, timeout: Duration) -> Result<()> {
    set("none", timeout)?;
    set(config, timeout)
}
