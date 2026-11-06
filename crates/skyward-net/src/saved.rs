//! The network the recovery last joined, kept where a later boot finds it.
//!
//! The supplicant's own settings live in a folder that a boot clears, so a
//! network joined by hand would be asked for again after every restart. This
//! is the one record kept: what the screen calls the network, how it is
//! protected, and the commands that set it up, written where the phone keeps
//! what outlives a boot.

use crate::supplicant::{self, Security};
use crate::wlan::Joined;
use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// The first line of a file this build wrote; one of another build is not
/// guessed at.
const FIRST_LINE: &str = "skyward-wifi 1";

/// The network an earlier boot joined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub name: String,
    pub ssid: Vec<u8>,
    pub security: Security,
    pub commands: Vec<String>,
}

impl Saved {
    /// The network set up from what the screen asked, with the commands that
    /// set it, ready to be kept.
    pub fn of(joined: &Joined, commands: &[String]) -> Saved {
        Saved {
            // The name is one line on the screen; a line break would end it.
            name: joined.name.replace(['\n', '\r'], " "),
            ssid: joined.ssid.clone(),
            security: joined.security,
            commands: commands.to_vec(),
        }
    }

    /// The network a file holds, if this build wrote it.
    pub fn read(text: &str) -> Option<Saved> {
        let mut lines = text.lines();
        if lines.next()? != FIRST_LINE {
            return None;
        }
        let (mut name, mut ssid, mut security) = (None, None, None);
        let mut commands = Vec::new();
        for line in lines {
            if let Some(value) = line.strip_prefix("name=") {
                name = Some(value.to_string());
            } else if let Some(value) = line.strip_prefix("ssid=") {
                ssid = Some(unhex(value)?);
            } else if let Some(value) = line.strip_prefix("security=") {
                security = Security::from_name(value);
            } else if line.starts_with("SET_NETWORK ") {
                commands.push(line.to_string());
            }
        }
        Some(Saved { name: name?, ssid: ssid?, security: security?, commands })
    }

    /// The network as the file holds it.
    pub fn text(&self) -> String {
        let mut text = format!("{FIRST_LINE}\nname={}\nssid={}\nsecurity={}\n", self.name, supplicant::hex(&self.ssid), self.security.name());
        for command in &self.commands {
            text.push_str(command);
            text.push('\n');
        }
        text
    }

    /// Write the network where the next boot finds it, readable by its user
    /// alone: it holds the password.
    pub fn write(&self, path: &Path) -> Result<()> {
        let folder = path.parent().context("the saved network has no folder")?;
        fs::create_dir_all(folder).with_context(|| format!("make {}", folder.display()))?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("write {}", path.display()))?;
        file.write_all(self.text().as_bytes()).with_context(|| format!("write {}", path.display()))?;
        file.sync_all().with_context(|| format!("flush {}", path.display()))
    }

    /// The commands as those that set up the network now numbered `id`, the
    /// number the supplicant gives a network it was just told to add.
    pub fn renumber(&self, id: u32) -> Result<Vec<String>> {
        self.commands.iter().map(|line| renumber(line, id)).collect()
    }
}

fn renumber(line: &str, id: u32) -> Result<String> {
    let rest = line.strip_prefix("SET_NETWORK ").context("a saved line sets no network")?;
    let (_old, value) = rest.split_once(' ').context("a saved line sets no value")?;
    Ok(format!("SET_NETWORK {id} {value}"))
}

/// The bytes a run of hex digits holds.
fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wlan::Stage;

    fn joined() -> Joined {
        Joined { ssid: b"home\nnetwork".to_vec(), name: "home network\n".into(), security: Security::Psk, stage: Stage::Joining }
    }

    fn saved() -> Saved {
        let commands = supplicant::network_commands(0, &joined().ssid, Security::PskSae, "a password", true).unwrap();
        Saved::of(&joined(), &commands)
    }

    #[test]
    fn a_saved_network_reads_back_as_it_was_written() {
        let saved = saved();
        let read = Saved::read(&saved.text()).expect("the file this build wrote reads back");
        assert_eq!(read, saved);
        assert_eq!(read.name, "home network ");
        assert_eq!(read.ssid, b"home\nnetwork");
    }

    #[test]
    fn the_notes_inside_a_password_survive() {
        let commands = supplicant::network_commands(0, b"n", Security::Psk, "quote \" inside", false).unwrap();
        let saved = Saved::of(&Joined { ssid: b"n".to_vec(), name: "n".into(), security: Security::Psk, stage: Stage::Joining }, &commands);
        assert_eq!(Saved::read(&saved.text()).unwrap().commands, saved.commands);
    }

    #[test]
    fn a_saved_command_takes_the_network_number_now_given() {
        assert_eq!(renumber("SET_NETWORK 0 psk \"secret\"", 3).unwrap(), "SET_NETWORK 3 psk \"secret\"");
        assert!(renumber("ADD_NETWORK", 3).is_err());
        assert_eq!(saved().renumber(7).unwrap()[0], "SET_NETWORK 7 ssid 686f6d650a6e6574776f726b");
    }

    #[test]
    fn a_file_of_another_build_is_refused() {
        assert!(Saved::read("skyward-wifi 2\nssid=00\nsecurity=psk\n").is_none());
        assert!(Saved::read("skyward-wifi 1\nssid=0\nsecurity=psk\n").is_none());
        assert!(Saved::read("skyward-wifi 1\nssid=00\nsecurity=none\n").is_none());
    }
}
