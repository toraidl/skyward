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
    if config == "mtp,adb" {
        // No rule in the recovery names both functions and says the port
        // is up, so this does it and then tells the property other
        // programs wait on.
        name_mtp(timeout)?;
        set_prop("sys.usb.state", config)?;
    }
    wait_prop("sys.usb.state", config, timeout)?;
    if config == "none" {
        let start = Instant::now();
        while !matches!(prop("init.svc.adbd").as_deref(), None | Some("stopped")) {
            ensure!(start.elapsed() < timeout, "adbd still runs {} ms after the port went to none", timeout.as_millis());
            thread::sleep(Duration::from_millis(10));
        }
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
    let until = Instant::now() + timeout;
    wait_for_property("init.svc.adbd", "running", until).context("wait for adbd before restoring MTP")?;
    wait_for_property("sys.usb.ffs.ready", "1", until).context("wait for adbd's FunctionFS descriptors")?;
    let mut last_error: Option<String>;
    loop {
        match name_mtp_once(&udc, &first, &second, &controller) {
            Ok(true) => return Ok(()),
            Ok(false) => last_error = Some("init put adb back in the first link".to_string()),
            Err(error) if retryable_configfs_error(&error) => last_error = Some(format!("{error:#}")),
            Err(error) => return Err(error),
        }
        ensure!(
            Instant::now() < until,
            "MTP/ADB USB setup did not settle within {} ms: {}",
            timeout.as_millis(),
            last_error.as_deref().unwrap_or("no link state observed")
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_property(name: &str, value: &str, until: Instant) -> Result<()> {
    loop {
        let current = prop(name);
        if current.as_deref() == Some(value) {
            return Ok(());
        }
        ensure!(Instant::now() < until, "{name} is {current:?}, not {value:?}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn name_mtp_once(udc: &Path, first: &Path, second: &Path, controller: &str) -> Result<bool> {
    fs::write(udc, "").context("take the port down")?;
    // init answers a port that went down with its own links again; let it,
    // then take both off before naming this recovery's.
    thread::sleep(Duration::from_millis(50));
    remove_config_link(first).context("remove the first USB function link")?;
    remove_config_link(second).context("remove the second USB function link")?;
    fs::write(Path::new(GADGET).join("idProduct"), "0x4EE2").context("name the port's product")?;
    fs::write(Path::new(CONFIG).join("strings/0x409/configuration"), "mtp_adb")
        .context("name the port's configuration")?;
    symlink(format!("{GADGET}/functions/ffs.mtp"), first).context("name the MTP function")?;
    symlink(format!("{GADGET}/functions/ffs.adb"), second).context("name the adb function")?;
    fs::write(udc, controller).context("give the port its configuration")?;
    thread::sleep(Duration::from_millis(50));
    Ok(fs::read_link(first).is_ok_and(|to| to.ends_with("functions/ffs.mtp")))
}

fn remove_config_link(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn retryable_configfs_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|error| {
            matches!(
                error.kind(),
                std::io::ErrorKind::AlreadyExists
                    | std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::WouldBlock
            ) || matches!(error.raw_os_error(), Some(libc::EBUSY) | Some(libc::ENODEV))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn configfs_link_and_udc_races_are_retried() {
        for errno in [libc::EEXIST, libc::EBUSY, libc::ENODEV] {
            let error = anyhow!(std::io::Error::from_raw_os_error(errno)).context("configure USB");
            assert!(retryable_configfs_error(&error), "errno {errno} should be retried");
        }
    }

    #[test]
    fn permanent_configfs_errors_are_not_retried() {
        let error = anyhow!(std::io::Error::from_raw_os_error(libc::EINVAL));
        assert!(!retryable_configfs_error(&error));
    }

    #[test]
    fn removing_an_already_absent_config_link_is_ok() {
        let path = std::env::temp_dir().join(format!("skyward-usb-link-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        remove_config_link(&path).unwrap();
    }
}

/// Hand the port to `config` through "none", which stops the programs
/// serving it and takes every function off; init then sets the port up
/// once the programs behind the new configuration are ready.
pub fn switch(config: &str, timeout: Duration) -> Result<()> {
    set("none", timeout)?;
    set(config, timeout)
}
