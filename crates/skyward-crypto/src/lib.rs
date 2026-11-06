pub mod fscrypt;
pub mod gatekeeper;
pub mod keystore;
pub mod locksettings;
pub mod sharedsecret;
pub mod token;
pub mod user;
pub mod weaver;

/// The part of a key blob the phone's key service takes.
///
/// Android's keystore2 prefixes every blob it hands out with `pKMblob` and
/// a byte saying whether a software key service made it, so that it can
/// send a software-made key to its own software service. The hardware
/// service knows nothing of the prefix and calls the blob invalid.
pub fn plain_key_blob(blob: &[u8]) -> anyhow::Result<&[u8]> {
    const MAGIC: &[u8] = b"pKMblob";
    if blob.len() < 8 || !blob.starts_with(MAGIC) || !matches!(blob[7], 0 | 1) {
        return Ok(blob);
    }
    if blob[7] == 1 {
        anyhow::bail!("the key was made by the software key service, which this recovery does not run");
    }
    Ok(&blob[8..])
}

#[cfg(target_os = "android")]
pub mod data;
#[cfg(target_os = "android")]
pub mod keymint;
#[cfg(target_os = "android")]
pub mod keymaster;
#[cfg(target_os = "android")]
pub mod keystore2;

#[cfg(target_os = "android")]
impl keystore::Decryptor for keymint::KeyMint<'_> {
    fn gcm_decrypt(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], body: &[u8]) -> anyhow::Result<Vec<u8>> {
        // The keys unwrapped this way belong to the phone rather than to
        // any user, so none of them is guarded by a screen lock.
        keymint::KeyMint::gcm_decrypt(self, key_blob, app_id, nonce, body, None)
    }
}
