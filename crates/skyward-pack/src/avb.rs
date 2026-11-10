//! The footer a recovery image ends in for the bootloader to verify it by:
//! an unsigned vbmeta image holding the image's digest, laid out over the
//! whole partition as avbtool's `add_hash_footer --algorithm NONE` lays it.
//! An unlocked bootloader boots such an image whatever its digest; it gives
//! up on the whole slot when the footer at the partition's end points at
//! anything but a vbmeta image, as a stock footer left behind an image
//! written over the start of the partition does.

use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};

/// The unit avbtool lays an image and its vbmeta image out in.
const BLOCK: usize = 4096;
const FOOTER_LEN: usize = 64;
const HEADER_LEN: usize = 256;
const TAG_HASH: u64 = 2;
const RELEASE: &[u8] = b"skyward-pack";

/// What a vbmeta header carries besides the digest of the image: the
/// release name, rollback bookkeeping, and the property descriptors the
/// bootloader hands on to the secure world.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub release: Vec<u8>,
    pub rollback_index: u64,
    pub flags: u32,
    pub rollback_index_location: u32,
    pub properties: Vec<u8>,
}

impl Header {
    /// The header `skyward-pack` has always written for an image whose
    /// stock half says nothing worth carrying over.
    fn skyward() -> Header {
        Header { release: RELEASE.to_vec(), ..Header::default() }
    }
}

/// Read the footer and vbmeta header of a whole partition, keeping the
/// property descriptors that describe the boot image. A stock recovery's
/// boot patch and OS version live here; without them the secure world sees
/// a recovery older than the key it is asked to open.
pub fn header(image: &[u8]) -> Result<Header> {
    ensure!(image.len() >= FOOTER_LEN, "the image is too short to hold an AVB footer");
    let footer = &image[image.len() - FOOTER_LEN..];
    ensure!(&footer[..4] == b"AVBf", "the image does not end in an AVB footer");
    let vbmeta_offset = be_u64(&footer[20..28]) as usize;
    let vbmeta_size = be_u64(&footer[28..36]) as usize;
    ensure!(vbmeta_offset.checked_add(vbmeta_size).is_some_and(|end| end <= image.len()), "the AVB footer points outside the image");
    let vbmeta = &image[vbmeta_offset..vbmeta_offset + vbmeta_size];
    ensure!(vbmeta.len() >= HEADER_LEN && &vbmeta[..4] == b"AVB0", "the AVB footer does not point at a vbmeta image");
    let auth = be_u64(&vbmeta[12..20]) as usize;
    let aux = be_u64(&vbmeta[20..28]) as usize;
    let descriptors_at = be_u64(&vbmeta[96..104]) as usize;
    let descriptors_len = be_u64(&vbmeta[104..112]) as usize;
    let aux_start = HEADER_LEN.checked_add(auth).ok_or_else(|| anyhow::anyhow!("the vbmeta header is too large"))?;
    ensure!(aux_start.checked_add(aux).is_some_and(|end| end <= vbmeta.len()), "the vbmeta auxiliary block is outside the image");
    ensure!(descriptors_at.checked_add(descriptors_len).is_some_and(|end| end <= aux), "the vbmeta descriptors are outside the auxiliary block");
    let descriptors = &vbmeta[aux_start + descriptors_at..aux_start + descriptors_at + descriptors_len];
    let properties = property_descriptors(descriptors)?;
    let release = vbmeta[128..176].split(|b| *b == 0).next().unwrap_or(&[]).to_vec();
    Ok(Header {
        release,
        rollback_index: be_u64(&vbmeta[112..120]),
        flags: be_u32(&vbmeta[120..124]),
        rollback_index_location: be_u32(&vbmeta[124..128]),
        properties,
    })
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().expect("a four-byte field"))
}

fn be_u64(bytes: &[u8]) -> u64 {
    u64::from_be_bytes(bytes.try_into().expect("an eight-byte field"))
}

/// Copy the property descriptors from a stock vbmeta image. The hash
/// descriptor is left behind because it describes the stock image, not the
/// one being written.
fn property_descriptors(mut descriptors: &[u8]) -> Result<Vec<u8>> {
    let mut properties = Vec::new();
    while !descriptors.is_empty() {
        ensure!(descriptors.len() >= 16, "a vbmeta descriptor is truncated");
        let tag = be_u64(&descriptors[..8]);
        let following = be_u64(&descriptors[8..16]) as usize;
        let size = 16usize.checked_add(following).ok_or_else(|| anyhow::anyhow!("a vbmeta descriptor is too large"))?;
        ensure!(size <= descriptors.len(), "a vbmeta descriptor runs past the descriptor block");
        if tag == 0 {
            properties.extend_from_slice(&descriptors[..size]);
        }
        descriptors = &descriptors[size..];
    }
    Ok(properties)
}

/// `image` grown to fill a `partition_size`-byte partition named
/// `partition`: the image, its vbmeta image from the next block on, and the
/// footer locating that in the partition's last bytes, zeros between.
pub fn footed(image: &[u8], partition: &str, partition_size: usize, salt: &[u8]) -> Result<Vec<u8>> {
    footed_with(image, partition, partition_size, salt, &Header::skyward())
}

/// As [`footed`], with the release name and property descriptors a stock
/// image carried.
pub fn footed_with(image: &[u8], partition: &str, partition_size: usize, salt: &[u8], header: &Header) -> Result<Vec<u8>> {
    let digest = Sha256::new().chain_update(salt).chain_update(image).finalize();
    let mut descriptors = hash_descriptor(image.len() as u64, partition, salt, &digest);
    descriptors.extend_from_slice(&header.properties);
    let vbmeta = vbmeta(&descriptors, header);
    let offset = image.len().next_multiple_of(BLOCK);
    let end = offset + vbmeta.len().next_multiple_of(BLOCK);
    ensure!(
        end + BLOCK <= partition_size,
        "a {}-byte image and its vbmeta image do not fit a {partition_size}-byte {partition} partition beside its footer",
        image.len()
    );
    let mut out = Vec::with_capacity(partition_size);
    out.extend_from_slice(image);
    out.resize(offset, 0);
    out.extend_from_slice(&vbmeta);
    out.resize(partition_size - FOOTER_LEN, 0);
    out.extend_from_slice(b"AVBf");
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    for n in [image.len(), offset, vbmeta.len()] {
        out.extend_from_slice(&(n as u64).to_be_bytes());
    }
    out.resize(partition_size, 0);
    Ok(out)
}

/// A descriptor saying the first `image_size` bytes of `partition`, salted
/// with `salt`, hash to `digest` under SHA-256.
fn hash_descriptor(image_size: u64, partition: &str, salt: &[u8], digest: &[u8]) -> Vec<u8> {
    let mut algorithm = [0u8; 32];
    algorithm[..6].copy_from_slice(b"sha256");
    let mut d = Vec::new();
    d.extend_from_slice(&TAG_HASH.to_be_bytes());
    // How many bytes follow these first sixteen, once they are known.
    d.extend_from_slice(&0u64.to_be_bytes());
    d.extend_from_slice(&image_size.to_be_bytes());
    d.extend_from_slice(&algorithm);
    // The lengths of the name, salt and digest, then the flags.
    for n in [partition.len(), salt.len(), digest.len(), 0] {
        d.extend_from_slice(&(n as u32).to_be_bytes());
    }
    d.extend_from_slice(&[0; 60]);
    d.extend_from_slice(partition.as_bytes());
    d.extend_from_slice(salt);
    d.extend_from_slice(digest);
    d.resize(d.len().next_multiple_of(8), 0);
    let following = (d.len() - 16) as u64;
    d[8..16].copy_from_slice(&following.to_be_bytes());
    d
}

/// An unsigned vbmeta image holding `descriptors`: its header, an empty
/// authentication block and the descriptors as its auxiliary block.
fn vbmeta(descriptors: &[u8], header: &Header) -> Vec<u8> {
    let aux = descriptors.len().next_multiple_of(64);
    let mut v = Vec::with_capacity(HEADER_LEN + aux);
    v.extend_from_slice(b"AVB0");
    // The libavb version it needs, 1.0.
    v.extend_from_slice(&1u32.to_be_bytes());
    v.extend_from_slice(&0u32.to_be_bytes());
    v.extend_from_slice(&0u64.to_be_bytes());
    v.extend_from_slice(&(aux as u64).to_be_bytes());
    // No signing algorithm.
    v.extend_from_slice(&0u32.to_be_bytes());
    // The offsets and sizes of the hash, the signature, the public key, its
    // metadata and the descriptors, then the rollback index.
    let n = descriptors.len();
    for field in [0, 0, 0, 0, n as u64, 0, n as u64, 0, 0, n as u64, header.rollback_index] {
        v.extend_from_slice(&field.to_be_bytes());
    }
    v.extend_from_slice(&header.flags.to_be_bytes());
    v.extend_from_slice(&header.rollback_index_location.to_be_bytes());
    let mut release = header.release.clone();
    release.truncate(47);
    v.extend_from_slice(&release);
    v.resize(HEADER_LEN, 0);
    v.extend_from_slice(descriptors);
    v.resize(HEADER_LEN + aux, 0);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const SALT: [u8; 32] = [0x5a; 32];

    /// What avbtool 1.4.0 makes of `len` bytes of `S`, given by
    /// `add_hash_footer --partition_size 81920 --partition_name recovery
    /// --algorithm NONE --salt 5a…5a --internal_release_string skyward-pack`.
    fn avbtool(len: usize) -> &'static [u8] {
        match len {
            4096 => include_bytes!("../tests/avb-footer-4096.bin"),
            1000 => include_bytes!("../tests/avb-footer-1000.bin"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn the_footer_is_avbtools_byte_for_byte() {
        for len in [4096, 1000] {
            let got = footed(&vec![b'S'; len], "recovery", 81920, &SALT).unwrap();
            assert!(got == avbtool(len), "a {len}-byte image");
        }
    }

    #[test]
    fn an_image_that_leaves_no_room_for_its_vbmeta_and_footer_is_refused() {
        let image = vec![b'S'; 8192];
        assert!(footed(&image, "recovery", 16384, &SALT).is_ok(), "a block each for the vbmeta image and the footer");
        assert!(footed(&image, "recovery", 12288, &SALT).is_err());
    }

    /// The property descriptor avbtool writes for a boot image: a tag, a
    /// length, the two string lengths, the strings, and their terminators.
    fn property(key: &str, value: &str) -> Vec<u8> {
        let mut d = vec![0u8; 16];
        d.extend_from_slice(&(key.len() as u64).to_be_bytes());
        d.extend_from_slice(&(value.len() as u64).to_be_bytes());
        d.extend_from_slice(key.as_bytes());
        d.push(0);
        d.extend_from_slice(value.as_bytes());
        d.push(0);
        d.resize(d.len().next_multiple_of(8), 0);
        let following = (d.len() - 16) as u64;
        d[8..16].copy_from_slice(&following.to_be_bytes());
        d
    }

    #[test]
    fn the_stock_headers_boot_properties_are_carried_over() {
        let mut properties = property("com.android.build.boot.os_version", "99.87.36");
        properties.extend(property("com.android.build.boot.security_patch", "2127-12-31"));
        let header = Header { release: b"stock-recovery".to_vec(), rollback_index: 1, flags: 0, rollback_index_location: 0, properties };
        let image = footed_with(&vec![b'S'; 1000], "recovery", 81920, &SALT, &header).unwrap();
        let read = self::header(&image).unwrap();
        assert_eq!(read.release, b"stock-recovery");
        assert_eq!(read.rollback_index, 1);
        assert_eq!(read.properties, header.properties);
        let key = b"com.android.build.boot.os_version";
        assert!(image.windows(key.len()).any(|w| w == key), "the boot version is in the footer");
    }
}
