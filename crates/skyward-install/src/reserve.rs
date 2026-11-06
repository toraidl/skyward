//! OnePlus keeps its reserve partition in a file inside user data instead
//! of on a partition of its own. The stock recovery puts that file and the
//! slot names pointing at it in place before an update starts: the update
//! engine writes the payload's `reserve` entry through them, and the
//! installed system mounts the file as a loop device on its next boot. A
//! missing file is why a phone that finished an update fails to mount
//! `/mnt/vendor/my_reserve`.

use anyhow::{Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Where the phone keeps the reserve partition as a file, and the folder
/// around it.
const IMAGE: &str = "/data/reserve/reserve.img";
const FOLDER: &str = "/data/reserve";

/// The names the update engine looks the file up by.
const BY_NAME: &str = "/dev/block/by-name";
const SLOTS: [&str; 2] = ["_a", "_b"];

/// Where the block maps the stock updater leaves in the cache are kept
/// after it finishes.
const MAPS: &str = "/mnt/vendor/oplusreserve/recovery";

/// The account that owns the folder: `system` on Android.
const SYSTEM: u32 = 1000;

/// Make sure the reserve file and its slot names are in place, as the
/// phone's own recovery does before it starts an update. A phone that does
/// not keep its reserve partition this way is left as it is.
pub fn prepare() -> Result<()> {
    if !data_mounted() {
        return Ok(());
    }
    let folder = Path::new(FOLDER);
    fs::create_dir_all(folder).with_context(|| format!("make {}", folder.display()))?;
    let _ = fs::set_permissions(folder, fs::Permissions::from_mode(0o771));
    let _ = std::os::unix::fs::chown(folder, Some(SYSTEM), Some(SYSTEM));

    let image = Path::new(IMAGE);
    if !image.exists() {
        fs::File::create(image).with_context(|| format!("make {}", image.display()))?;
        let _ = fs::set_permissions(image, fs::Permissions::from_mode(0o644));
    }

    // A phone that still has a real reserve partition keeps it.
    let real = Path::new(BY_NAME).join("reserve");
    if !real.exists() {
        for slot in SLOTS {
            let link = Path::new(BY_NAME).join(format!("reserve{slot}"));
            if link.exists() {
                continue;
            }
            std::os::unix::fs::symlink(image, &link).with_context(|| format!("link {}", link.display()))?;
        }
    }

    let maps = Path::new(MAPS);
    fs::create_dir_all(maps).with_context(|| format!("make {}", maps.display()))?;
    Ok(())
}

/// Whether the phone's user data is open, where the reserve file lives.
fn data_mounted() -> bool {
    fs::read_to_string("/proc/mounts").map(|mounts| mounted(&mounts)).unwrap_or(false)
}

/// Whether the mount table says user data is open.
fn mounted(mounts: &str) -> bool {
    mounts.lines().any(|line| line.split_whitespace().nth(1) == Some("/data"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_data_open_is_told_from_its_mount() {
        let open = "/dev/block/dm-15 /data f2fs rw,lazytime 0 0\n/dev/block/dm-15 /sdcard f2fs rw 0 0\n";
        assert!(mounted(open));
        let closed = "/dev/block/dm-15 /data/media f2fs rw 0 0\n";
        assert!(!mounted(closed));
    }
}
