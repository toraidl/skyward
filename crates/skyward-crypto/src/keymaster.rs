//! The phone's key service of the older kind, a hardware service reached
//! over the hardware driver rather than the service directory. Its calls
//! are laid out as the phone's own code lays them out: every vector is a
//! frame of a pointer, a count and whether its holder frees it, and every
//! structure held inside another is written into the other's room.
//!
//! The operations a recovery needs are the same as on a newer key service:
//! ask what a key may be used for, start a GCM decryption with it, feed it
//! and finish it. A storage key made for a newer service is exported raw,
//! which is what keystore2 asks of this one.

use crate::token::Token;
use anyhow::{bail, Context, Result};
use skyward_binder::driver::{Handle, Reply as BinderReply};
use skyward_binder::hidl::Message;

/// The phone's key service, whichever version of its interface it serves.
pub const SERVICES: [&str; 2] = [
    "android.hardware.keymaster@4.1::IKeymasterDevice",
    "android.hardware.keymaster@4.0::IKeymasterDevice",
];
const INSTANCE: &str = "default";
/// The name the calls this program makes are written under. Every one of
/// them is a method the 4.0 interface declares, and a service of the later
/// version takes them under that name.
const TOKEN: &str = "android.hardware.keymaster@4.0::IKeymasterDevice";

/// How each version numbers the calls this program makes. A service of the
/// later version keeps the numbers of the earlier one.
const GET_HARDWARE_INFO: u32 = 1;
const GET_KEY_CHARACTERISTICS: u32 = 9;
const EXPORT_KEY: u32 = 10;
const UPGRADE_KEY: u32 = 12;
const BEGIN: u32 = 16;
const UPDATE: u32 = 17;
const FINISH: u32 = 18;
const ABORT: u32 = 19;

/// How much of a key parameter is the structure around its value.
const PARAM_BYTES: usize = 32;
/// Where in that structure the number or the blob is.
const PARAM_VALUE: usize = 8;
const PARAM_BLOB: usize = 16;
/// Where in a frame the count of its elements is.
const FRAME_COUNT: usize = 8;

const AUTH_TOKEN_BYTES: usize = 56;
const AUTH_MAC: u64 = 40;
const VERIFICATION_TOKEN_BYTES: usize = 56;
const VERIFICATION_PARAMS: u64 = 16;
const VERIFICATION_MAC: u64 = 40;

const KEY_FORMAT_RAW: u32 = 3;
const PURPOSE_DECRYPT: u32 = 1;
const BLOCK_MODE_GCM: u64 = 32;

const TAG_BLOCK_MODE: i32 = 536_870_916;
const TAG_PADDING: i32 = 536_870_918;
const TAG_MAC_LENGTH: i32 = 805_307_371;
const TAG_APPLICATION_ID: i32 = -1_879_047_591;
const TAG_NONCE: i32 = -1_879_047_191;

const PADDING_NONE: u64 = 1;

/// The key service's way of saying the key was made under an older build of
/// the phone's own firmware and has to be renewed before it will be used.
const KEY_REQUIRES_UPGRADE: i32 = -62;

const NONCE_BYTES: usize = 12;

/// What the phone says about its key hardware.
pub struct Hardware {
    pub security_level: i32,
    pub name: String,
    pub author: String,
}

/// The phone's key service over its hardware interface.
pub struct Keymaster<'a> {
    device: Handle<'a>,
    interface: &'static str,
}

/// One authorization tag and what came with it.
type Parameter = (i32, i64, Vec<u8>);

/// One parameter a call carries: a tag, and either a number or a blob.
struct Param<'a> {
    tag: i32,
    value: Option<u64>,
    blob: Option<&'a [u8]>,
}

impl<'a> Keymaster<'a> {
    /// Reach whichever of the phone's key services is registered.
    pub fn open(hardware: &'a skyward_binder::driver::Binder) -> Result<Keymaster<'a>> {
        for interface in SERVICES {
            if let Some(device) = skyward_binder::hidl::find(hardware, interface, INSTANCE)? {
                return Ok(Keymaster { device, interface });
            }
        }
        bail!("no keymaster 4.0 or 4.1 service is registered; the phone's key service has to be started first")
    }

    pub fn interface(&self) -> &'static str {
        self.interface
    }

    /// What the service says it serves, which changes nothing.
    pub fn serves(&self) -> Result<String> {
        skyward_binder::hidl::describes(&self.device)
    }

    pub fn hardware(&self) -> Result<Hardware> {
        let reply = self.call(GET_HARDWARE_INFO, Message::call(TOKEN), "ask the key service about itself")?;
        let mut read = Reply::new(reply.bytes());
        read.status()?;
        let security_level = read.i32()?;
        let name = read.string()?;
        let author = read.string()?;
        Ok(Hardware { security_level, name, author })
    }

    /// The form of a storage key that lasts until the phone is started
    /// again, as a newer key service hands it over. The blob never leaves
    /// the secure world as itself: what comes back is wrapped for this
    /// start, for the storage hardware to unwrap as it reads and writes.
    pub fn ephemeral_storage_key(&self, key: &[u8]) -> Result<Vec<u8>> {
        match self.export(key) {
            Err(e) if refusal(&e) == Some(KEY_REQUIRES_UPGRADE) => {
                // The key service of a newer service renews the key with
                // nothing to say about how it will be used.
                let renewed = self.upgrade(key, &[]).context("renew the storage key for the firmware running now")?;
                self.export(&renewed)
            }
            exported => exported,
        }
    }

    fn export(&self, key: &[u8]) -> Result<Vec<u8>> {
        let key = crate::plain_key_blob(key)?;
        let mut call = Message::call(TOKEN);
        call.u32(KEY_FORMAT_RAW);
        call.bytes(key);
        call.bytes(&[]);
        call.bytes(&[]);
        let reply = self.call(EXPORT_KEY, call, "make a storage key for this start")?;
        let mut read = Reply::new(reply.bytes());
        read.status()?;
        read.error()?;
        read.vector()
    }

    /// What the key service says a key may be used for: each entry is a tag
    /// and the number or length that came with it.
    pub fn authorizations(&self, key_blob: &[u8], app_id: &[u8]) -> Result<Vec<(i32, i64)>> {
        let key_blob = crate::plain_key_blob(key_blob)?;
        let mut call = Message::call(TOKEN);
        call.bytes(key_blob);
        call.bytes(app_id);
        call.bytes(&[]);
        let reply = self.call(GET_KEY_CHARACTERISTICS, call, "ask what the key may be used for")?;
        let mut read = Reply::new(reply.bytes());
        read.status()?;
        read.error()?;
        // Both lists a key's authorizations are split into, in the order the
        // service writes them: the software enforces first.
        let characteristics = read.characteristics().context("read what the key may be used for")?;
        Ok(characteristics.into_iter().map(|(tag, value, _)| (tag, value)).collect())
    }

    /// Undo an AES-GCM encryption the key service did with `key_blob`. A key
    /// guarded by the screen lock takes the note the secure world wrote when
    /// the user answered it.
    pub fn gcm_decrypt(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], body: &[u8], note: Option<&Token>) -> Result<Vec<u8>> {
        let operation = match self.begin(key_blob, app_id, nonce, note) {
            Ok(operation) => operation,
            Err(e) if refusal(&e) == Some(KEY_REQUIRES_UPGRADE) => {
                // The phone's firmware has moved on since this key was made.
                // Renewing it gives a key for the firmware running now; the
                // one on disk is left as it is.
                let params = Self::gcm_params(app_id, nonce);
                let renewed = self.upgrade(key_blob, &params).context("renew the key for the firmware running now")?;
                self.begin(&renewed, app_id, nonce, note)?
            }
            Err(e) => return Err(e),
        };
        let mut out = self.update(operation, body)?;
        out.extend_from_slice(&self.finish(operation)?);
        Ok(out)
    }

    /// Whether the secure world will unwrap the key at all: a decryption is
    /// opened on it under a nonce of nothing and dropped again, which reads
    /// nothing and changes nothing.
    pub fn usable(&self, key_blob: &[u8], app_id: &[u8]) -> Result<()> {
        let operation = self.begin(key_blob, app_id, &[0; NONCE_BYTES], None)?;
        self.abort(operation)
    }

    /// What the key service has to be told to use an AES-GCM key.
    fn gcm_params<'b>(app_id: &'b [u8], nonce: &'b [u8]) -> Vec<Param<'b>> {
        let mut params = vec![
            Param { tag: TAG_BLOCK_MODE, value: Some(BLOCK_MODE_GCM), blob: None },
            Param { tag: TAG_PADDING, value: Some(PADDING_NONE), blob: None },
            Param { tag: TAG_MAC_LENGTH, value: Some(128), blob: None },
        ];
        if !app_id.is_empty() {
            params.push(Param { tag: TAG_APPLICATION_ID, value: None, blob: Some(app_id) });
        }
        params.push(Param { tag: TAG_NONCE, value: None, blob: Some(nonce) });
        params
    }

    fn begin(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], note: Option<&Token>) -> Result<u64> {
        let key_blob = crate::plain_key_blob(key_blob)?;
        let mut call = Message::call(TOKEN);
        call.u32(PURPOSE_DECRYPT);
        call.bytes(key_blob);
        let params = Self::gcm_params(app_id, nonce);
        Self::write_params(&mut call, &params);
        Self::write_auth_token(&mut call, note);
        let reply = self.call(BEGIN, call, "start a decryption")?;
        let mut read = Reply::new(reply.bytes());
        read.status()?;
        read.error()?;
        read.params().context("read what the decryption came back with")?;
        read.u64().context("read the decryption's handle")
    }

    fn update(&self, operation: u64, input: &[u8]) -> Result<Vec<u8>> {
        let mut call = Message::call(TOKEN);
        call.u64(operation);
        Self::write_params(&mut call, &[]);
        call.bytes(input);
        Self::write_auth_token(&mut call, None);
        Self::write_verification_token(&mut call);
        let reply = self.call(UPDATE, call, "feed the decryption")?;
        let mut read = Reply::new(reply.bytes());
        read.status()?;
        read.error()?;
        read.u32().context("read how much of the data was taken")?;
        read.params().context("read what the decryption came back with")?;
        read.vector().context("read what the decryption gave back")
    }

    fn finish(&self, operation: u64) -> Result<Vec<u8>> {
        let mut call = Message::call(TOKEN);
        call.u64(operation);
        Self::write_params(&mut call, &[]);
        call.bytes(&[]);
        call.bytes(&[]);
        Self::write_auth_token(&mut call, None);
        Self::write_verification_token(&mut call);
        let reply = self.call(FINISH, call, "close the decryption")?;
        let mut read = Reply::new(reply.bytes());
        read.status()?;
        read.error()?;
        read.params().context("read what the decryption came back with")?;
        read.vector().context("read what the decryption ended with")
    }

    fn abort(&self, operation: u64) -> Result<()> {
        let mut call = Message::call(TOKEN);
        call.u64(operation);
        let reply = self.call(ABORT, call, "drop the decryption")?;
        let mut read = Reply::new(reply.bytes());
        read.status()?;
        read.error().map(|_| ()).context("drop the decryption")
    }

    /// A key for the firmware running now, from one made under an older
    /// build. What it is bound to has to be given again, or the service
    /// cannot tell it is the right key.
    fn upgrade(&self, key_blob: &[u8], params: &[Param]) -> Result<Vec<u8>> {
        let key_blob = crate::plain_key_blob(key_blob)?;
        let mut call = Message::call(TOKEN);
        call.bytes(key_blob);
        Self::write_params(&mut call, params);
        let reply = self.call(UPGRADE_KEY, call, "renew the key")?;
        let mut read = Reply::new(reply.bytes());
        read.status()?;
        read.error()?;
        read.vector().context("read the renewed key")
    }

    // HIDL replies contain pointers into Binder-owned scatter/gather buffers.
    // Copying only the top-level bytes leaves dangling pointers when the
    // Binder reply is dropped. Keep it alive until each caller has decoded
    // and copied all nested strings, parameters and output bytes.
    fn call(&self, code: u32, message: Message, what: &str) -> Result<BinderReply<'a>> {
        self.device.transact(code, &message).with_context(|| what.to_string())
    }

    /// Write a vector of key parameters: its frame, the elements, and for
    /// each element the blob it may carry, which the frame's elements point
    /// at. A parameter with no blob still leaves the address of an empty
    /// one, as the phone's own code writes it.
    fn write_params(call: &mut Message, params: &[Param]) {
        let mut elements = vec![0u8; params.len() * PARAM_BYTES];
        for (index, param) in params.iter().enumerate() {
            let at = index * PARAM_BYTES;
            elements[at..at + 4].copy_from_slice(&param.tag.to_le_bytes());
            if let Some(value) = param.value {
                elements[at + PARAM_VALUE..at + PARAM_VALUE + 8].copy_from_slice(&value.to_le_bytes());
            }
            let blob = param.blob.unwrap_or(&[]);
            elements[at + PARAM_BLOB + 8..at + PARAM_BLOB + 12].copy_from_slice(&(blob.len() as u32).to_le_bytes());
        }
        let (_, elements_at) = call.vector_parts(&elements, params.len());
        for (index, param) in params.iter().enumerate() {
            let at = (index * PARAM_BYTES + PARAM_BLOB) as u64;
            call.part(param.blob.unwrap_or(&[]), elements_at, at);
        }
    }

    /// The note the secure world wrote when the user answered their screen
    /// lock, or a token saying none was asked for.
    fn write_auth_token(call: &mut Message, note: Option<&Token>) {
        let mut token = [0u8; AUTH_TOKEN_BYTES];
        let mac: &[u8] = match note {
            Some(note) => {
                token[0..8].copy_from_slice(&note.challenge.to_le_bytes());
                token[8..16].copy_from_slice(&note.user.to_le_bytes());
                token[16..24].copy_from_slice(&note.authenticator.to_le_bytes());
                token[24..28].copy_from_slice(&note.kind.to_le_bytes());
                token[32..40].copy_from_slice(&note.when.to_le_bytes());
                token[AUTH_MAC as usize + 8..AUTH_MAC as usize + 12].copy_from_slice(&(note.mac.len() as u32).to_le_bytes());
                &note.mac
            }
            None => &[],
        };
        let at = call.value(&token);
        call.part(mac, at, AUTH_MAC);
    }

    /// A token saying no other key service verified anything, which is what
    /// a caller that has none sends.
    fn write_verification_token(call: &mut Message) {
        let at = call.value(&[0u8; VERIFICATION_TOKEN_BYTES]);
        call.part(&[], at, VERIFICATION_PARAMS);
        call.part(&[], at, VERIFICATION_MAC);
    }
}

/// A refusal the key service named, so the caller can tell a key made for
/// an older build apart from a key it will not use at all.
fn refusal(error: &anyhow::Error) -> Option<i32> {
    error.downcast_ref::<Refused>().map(|r| r.0)
}

#[derive(Debug)]
struct Refused(i32);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the key service refused with its code {}", self.0)
    }
}

impl std::error::Error for Refused {}

/// Reads back what a hardware service answered. The values come in the
/// order the call's own signature gives them: numbers where they fall, and
/// addresses for everything the driver moved into this program's room.
struct Reply<'a> {
    data: &'a [u8],
    at: usize,
}

/// An address among a reply's values: the bytes it names and how many.
#[derive(Clone, Copy)]
struct Address {
    at: u64,
    len: u64,
}

/// How much room an address takes among the values.
const ADDRESS_BYTES: usize = 40;
const ADDRESS_MARK: u32 = u32::from_be_bytes([b'p', b't', b'*', 0x85]);
/// How much room a frame takes: the pointer, the count, and whether its
/// holder frees what it points at.
const FRAME_BYTES: usize = 16;

impl<'a> Reply<'a> {
    fn new(data: &'a [u8]) -> Reply<'a> {
        Reply { data, at: 0 }
    }

    /// Whether the call went through. A service that refused says so first,
    /// and says why in the text that follows.
    fn status(&mut self) -> Result<()> {
        let code = self.i32().context("read whether the service took the call")?;
        if code == 0 {
            return Ok(());
        }
        match self.text() {
            Ok(why) if !why.is_empty() => bail!("the service refused the call: {why}"),
            _ => bail!("the service refused the call with its code {code}"),
        }
    }

    /// The key service's own answer, which says whether it did what was
    /// asked and why not.
    fn error(&mut self) -> Result<()> {
        let code = self.i32().context("read what the key service answered")?;
        if code == 0 {
            Ok(())
        } else {
            Err(anyhow::Error::new(Refused(code)))
        }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at + n;
        if end > self.data.len() {
            bail!("the reply holds {} bytes, {n} more were read at {}", self.data.len(), self.at);
        }
        let out = &self.data[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn i32(&mut self) -> Result<i32> {
        self.at = self.at.div_ceil(4) * 4;
        Ok(i32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    fn u32(&mut self) -> Result<u32> {
        self.at = self.at.div_ceil(4) * 4;
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    fn u64(&mut self) -> Result<u64> {
        self.at = self.at.div_ceil(8) * 8;
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("eight bytes")))
    }

    /// Text as the phone's own code writes it: how many UTF-16 units, then
    /// those units and a terminator.
    fn text(&mut self) -> Result<String> {
        let units = self.i32()?;
        if units < 0 {
            return Ok(String::new());
        }
        let raw = self.take((units as usize + 1) * 2)?;
        let utf16: Vec<u16> = raw[..units as usize * 2].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        Ok(String::from_utf16_lossy(&utf16))
    }

    /// A string the phone's own code writes as a structure of its own: the
    /// length it has and, in a buffer of its own, its bytes and a
    /// terminator.
    fn string(&mut self) -> Result<String> {
        let string = self.address()?;
        let chars = self.address()?;
        let bytes = self.slice(chars);
        let length = if string.at == 0 {
            bytes.len()
        } else {
            let head = self.slice(Address { at: string.at, len: 16 });
            u32::from_le_bytes(head[8..12].try_into().expect("four bytes")) as usize
        };
        Ok(String::from_utf8_lossy(&bytes[..length.min(bytes.len())]).into_owned())
    }

    /// An address among the values.
    fn address(&mut self) -> Result<Address> {
        self.at = self.at.div_ceil(8) * 8;
        let raw = self.take(ADDRESS_BYTES)?;
        let kind = u32::from_le_bytes(raw[0..4].try_into().expect("four bytes"));
        if kind != ADDRESS_MARK {
            bail!("the reply holds {kind:#x} where an address was expected");
        }
        Ok(Address {
            at: u64::from_le_bytes(raw[8..16].try_into().expect("eight bytes")),
            len: u64::from_le_bytes(raw[16..24].try_into().expect("eight bytes")),
        })
    }

    /// The bytes an address names.
    fn slice(&self, address: Address) -> &'a [u8] {
        if address.at == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(address.at as *const u8, address.len as usize) }
    }

    /// A vector the service answered with: its frame first, then the
    /// elements themselves.
    fn vector(&mut self) -> Result<Vec<u8>> {
        let frame = self.address()?;
        let elements = self.address()?;
        let _ = frame;
        Ok(self.slice(elements).to_vec())
    }

    /// The pointer and count a frame holds.
    fn frame_fields(&self, frame: Address) -> Option<(usize, u64)> {
        if frame.at == 0 || frame.len < FRAME_BYTES as u64 {
            return None;
        }
        let raw = self.slice(Address { at: frame.at, len: FRAME_BYTES as u64 });
        let at = u64::from_le_bytes(raw[..8].try_into().ok()?);
        let count = u32::from_le_bytes(raw[FRAME_COUNT..FRAME_COUNT + 4].try_into().ok()?);
        Some((count as usize, at))
    }

    /// A vector of key parameters, in the order the service wrote them: its
    /// frame, the elements, then the blob each element carries.
    fn params(&mut self) -> Result<Vec<Parameter>> {
        let frame = self.address()?;
        let elements = self.address()?;
        let (count, _) = self.frame_fields(frame).context("read the frame of the parameters")?;
        let elements = Address { at: elements.at, len: elements.len };
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            let at = elements.at + (index * PARAM_BYTES) as u64;
            let base = self.slice(Address { at, len: PARAM_BYTES as u64 });
            if base.len() < PARAM_BYTES {
                bail!("the reply's parameters run past the end of what was moved");
            }
            let tag = i32::from_le_bytes(base[..4].try_into().expect("four bytes"));
            let value = u64::from_le_bytes(base[PARAM_VALUE..PARAM_VALUE + 8].try_into().expect("eight bytes"));
            let length = u32::from_le_bytes(base[PARAM_BLOB + 8..PARAM_BLOB + 12].try_into().expect("four bytes"));
            let blob_address = self.address()?;
            let blob = match length {
                0 => Vec::new(),
                _ => self.slice(blob_address).to_vec(),
            };
            out.push((tag, value as i64, blob));
        }
        Ok(out)
    }

    /// The one structure `getKeyCharacteristics` answers with, whose two
    /// lists are the software-enforced and hardware-enforced tags.
    fn characteristics(&mut self) -> Result<Vec<Parameter>> {
        let structure = self.address()?;
        if structure.len < 32 {
            bail!("the reply's key characteristics are {} bytes", structure.len);
        }
        let raw = self.slice(Address { at: structure.at, len: 32 });
        let mut out = Vec::new();
        for offset in [0usize, 16] {
            let count = u32::from_le_bytes(raw[offset + 8..offset + 12].try_into().expect("four bytes")) as usize;
            let elements = self.address()?;
            let elements = Address { at: elements.at, len: elements.len };
            for index in 0..count {
                let base_at = elements.at + (index * PARAM_BYTES) as u64;
                let base = self.slice(Address { at: base_at, len: PARAM_BYTES as u64 });
                if base.len() < PARAM_BYTES {
                    bail!("the reply's key characteristics run past the end of what was moved");
                }
                let tag = i32::from_le_bytes(base[..4].try_into().expect("four bytes"));
                let value = u64::from_le_bytes(base[PARAM_VALUE..PARAM_VALUE + 8].try_into().expect("eight bytes")) as i64;
                let length = u32::from_le_bytes(base[PARAM_BLOB + 8..PARAM_BLOB + 12].try_into().expect("four bytes"));
                let blob_address = self.address()?;
                let blob = match length {
                    0 => Vec::new(),
                    _ => self.slice(blob_address).to_vec(),
                };
                out.push((tag, value, blob));
            }
        }
        Ok(out)
    }
}
