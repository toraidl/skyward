use abootimg_oxide::{Header, HeaderV0, HeaderV0Versioned};
use anyhow::{ensure, Context, Result};
use sha1::{Digest, Sha1};
use std::fs;
use std::path::Path;

/// The page every version 3 and 4 section starts on; those headers have no
/// page size field of their own, and the format fixes it.
const PAGE_V3: usize = 4096;

/// The parts of a boot image, in the order the bootloader finds them.
///
/// Header versions 0, 1 and 2 keep the kernel, ramdisk, second stage
/// bootloader, recovery DTBO and DTB in sections of their own, sized and
/// placed by the header. Versions 3 and 4 keep only the kernel and ramdisk,
/// everything else having moved to `vendor_boot`.
pub struct BootImage {
    pub header: Header,
    pub kernel: Vec<u8>,
    pub ramdisk: Vec<u8>,
    pub second: Vec<u8>,
    pub recovery_dtbo: Vec<u8>,
    pub dtb: Vec<u8>,
}

fn section(bytes: &[u8], at: usize, len: u32, what: &str) -> Result<Vec<u8>> {
    let end = at + len as usize;
    bytes.get(at..end).map(<[u8]>::to_vec).ok_or_else(|| anyhow::anyhow!("{what} runs past the end of the image"))
}

impl BootImage {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let header = Header::parse(&mut abootimg_oxide::binrw::io::Cursor::new(&bytes)).with_context(|| format!("{}: header", path.display()))?;
        let kernel = section(&bytes, header.kernel_position(), header.kernel_size(), "kernel")?;
        let ramdisk = section(&bytes, header.ramdisk_position(), header.ramdisk_size(), "ramdisk")?;
        let (second, recovery_dtbo, dtb) = match &header {
            Header::V0(h) => legacy_sections(&bytes, h)?,
            Header::V3(_) => (Vec::new(), Vec::new(), Vec::new()),
        };
        Ok(BootImage { header, kernel, ramdisk, second, recovery_dtbo, dtb })
    }

    pub fn header_version(&self) -> u32 {
        self.header.header_version()
    }

    pub fn osversionpatch(&self) -> abootimg_oxide::OsVersionPatch {
        self.header.osversionpatch()
    }

    /// The image with sizes taken from its parts and its sections laid out
    /// again: each starts on a page boundary, as the bootloader expects.
    /// A version 2 header's hash covers every section it names. A version 4
    /// header keeps its version but carries no boot signature; unlocked
    /// bootloaders do not ask for one.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        match &self.header {
            Header::V3(h) => {
                let mut header = h.clone();
                header.kernel_size = u32::try_from(self.kernel.len()).context("kernel size")?;
                header.ramdisk_size = u32::try_from(self.ramdisk.len()).context("ramdisk size")?;
                header.v4_signature_size = header.v4_signature_size.map(|_| 0);
                Header::V3(header).write(&mut out).context("write header")?;
                pad(&mut out, PAGE_V3);
                out.extend_from_slice(&self.kernel);
                pad(&mut out, PAGE_V3);
                out.extend_from_slice(&self.ramdisk);
                pad(&mut out, PAGE_V3);
            }
            Header::V0(h) => {
                let mut header = h.clone();
                let page = header.page_size as usize;
                ensure!(page.is_power_of_two() && page >= 512, "page size {page} is not a power of two");
                header.kernel_size = u32::try_from(self.kernel.len()).context("kernel size")?;
                header.ramdisk_size = u32::try_from(self.ramdisk.len()).context("ramdisk size")?;
                header.second_bootloader_size = u32::try_from(self.second.len()).context("second stage bootloader size")?;
                match &mut header.versioned {
                    HeaderV0Versioned::V0 => {
                        ensure!(self.recovery_dtbo.is_empty() && self.dtb.is_empty(), "a version 0 header holds no recovery DTBO or DTB");
                    }
                    HeaderV0Versioned::V1 { recovery_dtbo_size, .. } => {
                        ensure!(self.dtb.is_empty(), "a version 1 header holds no DTB");
                        *recovery_dtbo_size = u32::try_from(self.recovery_dtbo.len()).context("recovery DTBO size")?;
                    }
                    HeaderV0Versioned::V2 { recovery_dtbo_size, dtb_size, .. } => {
                        *recovery_dtbo_size = u32::try_from(self.recovery_dtbo.len()).context("recovery DTBO size")?;
                        *dtb_size = u32::try_from(self.dtb.len()).context("DTB size")?;
                    }
                }
                header.hash_digest = self.legacy_hash();
                Header::V0(header).write(&mut out).context("write header")?;
                pad(&mut out, page);
                for part in [&self.kernel, &self.ramdisk, &self.second, &self.recovery_dtbo, &self.dtb] {
                    out.extend_from_slice(part);
                    pad(&mut out, page);
                }
            }
        }
        Ok(out)
    }

    /// What a version 0-2 header records of its sections: the SHA-1 of each
    /// part followed by its size as the bootloader reads them, with the
    /// unused bytes of the field left zero, as mkbootimg writes it.
    fn legacy_hash(&self) -> [u8; 32] {
        let mut hasher = Sha1::new();
        for part in [&self.kernel, &self.ramdisk, &self.second, &self.recovery_dtbo, &self.dtb] {
            if !part.is_empty() {
                hasher.update(part);
                hasher.update((part.len() as u32).to_le_bytes());
            } else {
                // A part the layout does not name is hashed as the one
                // mkbootimg hashes when it has none: nothing at all.
                hasher.update(0u32.to_le_bytes());
            }
        }
        let mut out = [0u8; 32];
        out[..20].copy_from_slice(&hasher.finalize());
        out
    }
}

/// The sections a version 0, 1 or 2 header lays out after the kernel and
/// ramdisk.
fn legacy_sections(bytes: &[u8], header: &HeaderV0) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let second = section(bytes, header.second_bootloader_position(), header.second_bootloader_size, "second stage bootloader")?;
    let (recovery_dtbo, dtb) = match header.versioned {
        HeaderV0Versioned::V0 => (Vec::new(), Vec::new()),
        HeaderV0Versioned::V1 { recovery_dtbo_size, .. } => {
            (section(bytes, header.recovery_dtbo_position(), recovery_dtbo_size, "recovery DTBO")?, Vec::new())
        }
        HeaderV0Versioned::V2 { recovery_dtbo_size, dtb_size, .. } => {
            let dtb = header.dtb_position().context("a version 2 header places its DTB")?;
            (
                section(bytes, header.recovery_dtbo_position(), recovery_dtbo_size, "recovery DTBO")?,
                section(bytes, dtb, dtb_size, "DTB")?,
            )
        }
    };
    Ok((second, recovery_dtbo, dtb))
}

fn pad(out: &mut Vec<u8>, page: usize) {
    out.resize(out.len().div_ceil(page) * page, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A version 2 image, as the bootloader lays one out: a header page,
    /// then each section on a page of its own.
    fn v2_image() -> Vec<u8> {
        let (kernel, ramdisk, dtbo, dtb) = (vec![1u8; 5000], vec![2u8; 8000], vec![3u8; 37], vec![4u8; 9000]);
        let header = HeaderV0 {
            kernel_size: 5000,
            kernel_addr: 0x8000,
            ramdisk_size: 8000,
            ramdisk_addr: 0x1000000,
            second_bootloader_size: 0,
            second_bootloader_addr: 0,
            tags_addr: 0x100,
            page_size: 4096,
            osversionpatch: abootimg_oxide::OsVersionPatch(0),
            board_name: [0; 16],
            hash_digest: [0; 32],
            cmdline: Box::new([0; 512 + 1024]),
            versioned: HeaderV0Versioned::V2 {
                recovery_dtbo_size: 37,
                recovery_dtbo_addr: 0x5414000,
                dtb_size: 9000,
                dtb_addr: 0x1f00000,
            },
        };
        BootImage { header: Header::V0(header), kernel, ramdisk, second: Vec::new(), recovery_dtbo: dtbo, dtb }.to_bytes().unwrap()
    }

    #[test]
    fn a_version_2_image_survives_a_round_trip() {
        let bytes = v2_image();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recovery.img");
        fs::write(&path, &bytes).unwrap();
        let image = BootImage::read(&path).unwrap();
        assert_eq!(image.header_version(), 2);
        assert_eq!(image.kernel.len(), 5000);
        assert_eq!(image.ramdisk.len(), 8000);
        assert_eq!(image.recovery_dtbo.len(), 37);
        assert_eq!(image.dtb.len(), 9000);
        assert_eq!(image.to_bytes().unwrap(), bytes);
    }

    #[test]
    fn the_version_2_hash_covers_every_section_with_its_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recovery.img");
        fs::write(&path, v2_image()).unwrap();
        let image = BootImage::read(&path).unwrap();
        let mut hasher = Sha1::new();
        for part in [&image.kernel, &image.ramdisk, &image.second, &image.recovery_dtbo, &image.dtb] {
            if !part.is_empty() {
                hasher.update(part);
                hasher.update((part.len() as u32).to_le_bytes());
            } else {
                hasher.update(0u32.to_le_bytes());
            }
        }
        let Header::V0(header) = &image.header else { unreachable!() };
        assert_eq!(&header.hash_digest[..20], hasher.finalize().as_slice());
        assert_eq!(&header.hash_digest[20..], &[0u8; 12]);
    }
}
