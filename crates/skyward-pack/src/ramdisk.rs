use anyhow::{bail, Context, Result};
use cpio::{NewcBuilder, NewcReader};
use std::io::{Cursor, Read, Write};

/// One cpio entry with the metadata the kernel restores when it unpacks the
/// archive into rootfs.
#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub mtime: u32,
    pub nlink: u32,
    pub dev: (u32, u32),
    pub rdev: (u32, u32),
    pub data: Vec<u8>,
}

const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;

/// The magic of an LZ4 legacy frame, little-endian.
const LZ4_LEGACY_MAGIC: u32 = 0x184C_2102;
const LZ4_FRAME_MAGIC: [u8; 4] = [0x04, 0x22, 0x4d, 0x18];
const XZ_MAGIC: [u8; 6] = [0xfd, b'7', b'z', b'X', b'Z', 0x00];
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];
const CPIO_MAGIC: [u8; 6] = *b"070701";

/// How much one LZ4 legacy block unpacks to.
const LZ4_BLOCK: usize = 8 * 1024 * 1024;

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.mode & S_IFMT == S_IFDIR
    }
}

/// An initramfs held in memory, so entries keep their owners, modes and
/// device numbers exactly as the archive had them.
pub struct Ramdisk {
    pub entries: Vec<Entry>,
    /// How the archive was found compressed, so it is packed that way again:
    /// a recovery's kernel bring-up expects the framing its own image used,
    /// and repacking another way would only be a needless change.
    pub compression: Compression,
}

/// The ways a recovery ramdisk is found compressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    /// No compression at all, a bare newc archive.
    None,
    /// The framing the kernel's LZ4 initramfs unpacker reads: a magic, then
    /// blocks of at most 8 MiB, each with its compressed length.
    Lz4Legacy,
    /// The LZ4 frame format, which some third-party tools write.
    Lz4Frame,
    Gzip,
    /// The legacy `.lzma` stream, as Qualcomm and OPPO stock recoveries
    /// carry their ramdisks.
    Lzma,
    Xz,
}

impl Ramdisk {
    /// Unpack a compressed newc archive, in whichever of the framings the
    /// kernel's initramfs unpacker reads the image uses.
    pub fn decode(compressed: &[u8]) -> Result<Self> {
        let compression = Compression::of(compressed)?;
        let cpio = decompress(compression, compressed)?;
        let mut entries = Vec::new();
        let mut rest = Cursor::new(cpio.as_slice());
        loop {
            let mut r = NewcReader::new(rest).context("cpio entry")?;
            if r.entry().is_trailer() {
                break;
            }
            let e = r.entry();
            let mut entry = Entry {
                name: e.name().to_string(),
                mode: e.mode(),
                uid: e.uid(),
                gid: e.gid(),
                mtime: e.mtime(),
                nlink: e.nlink(),
                dev: (e.dev_major(), e.dev_minor()),
                rdev: (e.rdev_major(), e.rdev_minor()),
                data: Vec::with_capacity(e.file_size() as usize),
            };
            r.read_to_end(&mut entry.data).with_context(|| format!("cpio data of {}", entry.name))?;
            entries.push(entry);
            rest = r.finish().context("cpio entry end")?;
        }
        Ok(Ramdisk { entries, compression })
    }

    /// Pack as a newc archive compressed the way the image it came from was.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut cpio = Vec::new();
        for (ino, e) in self.entries.iter().enumerate() {
            let size = u32::try_from(e.data.len()).with_context(|| format!("{} is too large", e.name))?;
            let mut w = NewcBuilder::new(&e.name)
                .ino(300_000 + ino as u32)
                .mode(e.mode)
                .uid(e.uid)
                .gid(e.gid)
                .mtime(e.mtime)
                .nlink(e.nlink)
                .dev_major(e.dev.0)
                .dev_minor(e.dev.1)
                .rdev_major(e.rdev.0)
                .rdev_minor(e.rdev.1)
                .write(&mut cpio, size);
            w.write_all(&e.data)?;
            w.finish()?;
        }
        cpio::newc::trailer(&mut cpio)?;
        compress(self.compression, &cpio)
    }

    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Replace a regular file's contents, keeping its owner and mode.
    pub fn replace(&mut self, name: &str, data: Vec<u8>) -> Result<()> {
        match self.entries.iter_mut().find(|e| e.name == name) {
            Some(e) if e.mode & S_IFMT == S_IFREG => {
                e.data = data;
                Ok(())
            }
            Some(_) => bail!("{name} is not a regular file"),
            None => bail!("{name} is not in the ramdisk"),
        }
    }

    /// Add a root-owned regular file, creating missing parent directories.
    pub fn add(&mut self, name: &str, mode: u32, data: Vec<u8>) -> Result<()> {
        if self.find(name).is_some() {
            bail!("{name} is already in the ramdisk");
        }
        let parts: Vec<&str> = name.split('/').collect();
        let mut parent = String::new();
        for part in &parts[..parts.len() - 1] {
            if !parent.is_empty() {
                parent.push('/');
            }
            parent.push_str(part);
            match self.find(&parent) {
                Some(e) if e.is_dir() => {}
                Some(_) => bail!("{parent} is not a directory"),
                None => self.entries.push(root_entry(&parent, S_IFDIR | 0o755, Vec::new())),
            }
        }
        self.entries.push(root_entry(name, S_IFREG | mode, data));
        Ok(())
    }
}

fn root_entry(name: &str, mode: u32, data: Vec<u8>) -> Entry {
    let nlink = if mode & S_IFMT == S_IFDIR { 2 } else { 1 };
    Entry { name: name.to_string(), mode, uid: 0, gid: 0, mtime: 0, nlink, dev: (0, 0), rdev: (0, 0), data }
}

impl Compression {
    /// The framing `bytes` starts with.
    pub fn of(bytes: &[u8]) -> Result<Self> {
        let starts = |magic: &[u8]| bytes.get(..magic.len()) == Some(magic);
        if starts(&GZIP_MAGIC) {
            Ok(Compression::Gzip)
        } else if starts(&LZ4_FRAME_MAGIC) {
            Ok(Compression::Lz4Frame)
        } else if bytes.len() >= 4 && u32::from_le_bytes(bytes[..4].try_into().expect("four bytes")) == LZ4_LEGACY_MAGIC {
            Ok(Compression::Lz4Legacy)
        } else if starts(&XZ_MAGIC) {
            Ok(Compression::Xz)
        } else if starts(&CPIO_MAGIC) || starts(b"070702") {
            Ok(Compression::None)
        } else if bytes.first().is_some_and(|p| *p <= 224) && bytes.get(1..4).is_some_and(|d| d.iter().any(|b| *b != 0)) {
            // The legacy LZMA stream opens with its properties byte and the
            // dictionary size; a property byte is never above 224. A stream
            // whose dictionary size is all zero is not one, whatever its
            // first byte says.
            Ok(Compression::Lzma)
        } else if starts(&[0x28, 0xb5, 0x2f, 0xfd]) {
            bail!("the ramdisk is Zstandard-compressed, which is not supported")
        } else {
            bail!("the ramdisk is not in a compression this packer knows: LZ4, gzip, LZMA or xz")
        }
    }
}

fn decompress(compression: Compression, input: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    match compression {
        Compression::None => out.extend_from_slice(input),
        Compression::Lz4Legacy => out = lz4_legacy_decode(input)?,
        Compression::Lz4Frame => {
            lz4_flex::frame::FrameDecoder::new(input).read_to_end(&mut out).context("lz4 frame decode")?;
        }
        Compression::Gzip => {
            flate2::read::GzDecoder::new(input).read_to_end(&mut out).context("gzip decode")?;
        }
        Compression::Lzma => {
            let stream = liblzma::stream::Stream::new_lzma_decoder(u64::MAX).context("start an LZMA decoder")?;
            liblzma::read::XzDecoder::new_stream(input, stream).read_to_end(&mut out).context("lzma decode")?;
        }
        Compression::Xz => {
            liblzma::read::XzDecoder::new(input).read_to_end(&mut out).context("xz decode")?;
        }
    }
    Ok(out)
}

fn compress(compression: Compression, input: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    match compression {
        Compression::None => out.extend_from_slice(input),
        Compression::Lz4Legacy => out = lz4_legacy_encode(input)?,
        Compression::Lz4Frame => {
            let mut w = lz4_flex::frame::FrameEncoder::new(&mut out);
            w.write_all(input).context("lz4 frame encode")?;
            w.finish().context("lz4 frame end")?;
        }
        Compression::Gzip => {
            let mut w = flate2::write::GzEncoder::new(&mut out, flate2::Compression::best());
            w.write_all(input).context("gzip encode")?;
            w.finish().context("gzip end")?;
        }
        Compression::Lzma => {
            let options = liblzma::stream::LzmaOptions::new_preset(6).context("set up LZMA")?;
            let stream = liblzma::stream::Stream::new_lzma_encoder(&options).context("start an LZMA encoder")?;
            let mut w = liblzma::write::XzEncoder::new_stream(&mut out, stream);
            w.write_all(input).context("lzma encode")?;
            w.finish().context("lzma end")?;
        }
        Compression::Xz => {
            let mut w = liblzma::write::XzEncoder::new(&mut out, 6);
            w.write_all(input).context("xz encode")?;
            w.finish().context("xz end")?;
        }
    }
    Ok(out)
}

/// An LZ4 legacy stream: the magic, then blocks of at most 8 MiB, each of
/// them a length and the block compressed on its own, then a zero length.
fn lz4_legacy_encode(input: &[u8]) -> Result<Vec<u8>> {
    let mut out = LZ4_LEGACY_MAGIC.to_le_bytes().to_vec();
    for block in input.chunks(LZ4_BLOCK) {
        let mut packed = vec![0u8; lz4_flex::block::get_maximum_output_size(block.len())];
        let len = lz4_flex::block::compress_into(block, &mut packed).context("lz4 block encode")?;
        out.extend_from_slice(&(len as u32).to_le_bytes());
        out.extend_from_slice(&packed[..len]);
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    Ok(out)
}

fn lz4_legacy_decode(input: &[u8]) -> Result<Vec<u8>> {
    let mut at = 4;
    let mut out = Vec::new();
    loop {
        let len = input.get(at..at + 4).context("an LZ4 legacy block length")?;
        at += 4;
        let len = u32::from_le_bytes(len.try_into().expect("four bytes")) as usize;
        if len == 0 {
            break;
        }
        let block = input.get(at..at + len).context("an LZ4 legacy block")?;
        at += len;
        let mut plain = vec![0u8; LZ4_BLOCK];
        let written = lz4_flex::block::decompress_into(block, &mut plain).context("lz4 block decode")?;
        out.extend_from_slice(&plain[..written]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_metadata() {
        let mut rd = Ramdisk { entries: Vec::new(), compression: Compression::Lz4Legacy };
        rd.add("system/bin/recovery", 0o750, b"binary".to_vec()).unwrap();
        rd.entries.push(Entry {
            name: "dev/console".into(),
            mode: 0o020600,
            uid: 0,
            gid: 0,
            mtime: 0,
            nlink: 1,
            dev: (0, 0),
            rdev: (5, 1),
            data: Vec::new(),
        });
        let back = Ramdisk::decode(&rd.encode().unwrap()).unwrap();
        let names: Vec<&str> = back.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["system", "system/bin", "system/bin/recovery", "dev/console"]);
        let bin = back.find("system/bin/recovery").unwrap();
        assert_eq!((bin.mode, bin.data.as_slice()), (S_IFREG | 0o750, b"binary".as_slice()));
        assert_eq!(back.find("dev/console").unwrap().rdev, (5, 1));
        assert!(rd.add("system/bin/recovery", 0o644, Vec::new()).is_err());
    }

    #[test]
    fn every_codec_round_trips() {
        for compression in [Compression::None, Compression::Lz4Legacy, Compression::Lz4Frame, Compression::Gzip, Compression::Lzma, Compression::Xz] {
            let mut rd = Ramdisk { entries: Vec::new(), compression };
            rd.add("system/bin/recovery", 0o750, b"binary".repeat(4096)).unwrap();
            let packed = rd.encode().unwrap();
            assert_eq!(Compression::of(&packed).unwrap(), compression, "{compression:?}");
            let back = Ramdisk::decode(&packed).unwrap();
            assert_eq!(back.compression, compression);
            assert_eq!(back.find("system/bin/recovery").unwrap().data, b"binary".repeat(4096));
        }
    }
}
