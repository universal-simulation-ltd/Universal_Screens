//! Who this device is, and which devices it has paired with — the memory that
//! lets a second connection skip the code.
//!
//! Each device holds one long-term **identity**: an X25519 key pair, the Noise
//! "static" key. Pairing (the code, run through SPAKE2) proves to both ends that
//! the other knows the code, and inside that proof each learns the other's
//! public key. From then on the keys alone are enough: a remembered device
//! connects with the `XX` handshake, each end checks the other's key against its
//! list, and no code is involved — which is also why a later connection gives an
//! eavesdropper nothing about the code at all.
//!
//! ## On disk
//!
//! A small text file, one record per line, so a person can read what is in it:
//!
//! ```text
//! # Universal Screens pairing keys. Private: it holds this device's secret key.
//! version 1
//! identity <secret, 64 hex> <public, 64 hex>
//! peer <public, 64 hex> <paired at, unix seconds> <label, rest of line>
//! ```
//!
//! ⚠️ **It holds a secret key.** It is written `0600` on unix and lives in the
//! per-user config folder on Windows. Anyone who copies it can impersonate this
//! device to every peer that trusts it — so "Forget paired devices" on the
//! other end is the remedy, and it is why the host has that button.
//!
//! A file that will not parse is **not** silently replaced (that would quietly
//! un-pair everything): opening it fails, and the caller falls back to an
//! in-memory store for this run.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Length of an X25519 key, public or secret.
pub const KEY_LEN: usize = 32;

/// A device's long-term Noise static key pair.
#[derive(Clone)]
pub struct Identity {
    secret: [u8; KEY_LEN],
    public: [u8; KEY_LEN],
}

impl Identity {
    /// A fresh key pair from the OS random source.
    ///
    /// # Errors
    /// If the Noise resolver cannot produce a key pair (no randomness).
    pub fn generate() -> io::Result<Identity> {
        let params = crate::handshake::known_params()?;
        let kp = snow::Builder::new(params).generate_keypair().map_err(crate::noise_err)?;
        let mut secret = [0u8; KEY_LEN];
        let mut public = [0u8; KEY_LEN];
        secret.copy_from_slice(&kp.private);
        public.copy_from_slice(&kp.public);
        Ok(Identity { secret, public })
    }

    /// Rebuild an identity from its two halves, as stored.
    ///
    /// ⚠️ The pair is trusted as given; a mismatched pair simply fails every
    /// handshake. Only [`PairingStore`] and the browser's storage call this,
    /// with what [`Identity::generate`] produced.
    #[must_use]
    pub fn from_parts(secret: [u8; KEY_LEN], public: [u8; KEY_LEN]) -> Identity {
        Identity { secret, public }
    }

    /// The public half — what peers remember.
    #[must_use]
    pub fn public(&self) -> [u8; KEY_LEN] {
        self.public
    }

    /// The secret half. Only the handshake and the store need it.
    #[must_use]
    pub fn secret(&self) -> [u8; KEY_LEN] {
        self.secret
    }
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Identity({})", short(&self.public))
    }
}

/// A device this one has paired with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedPeer {
    /// Its public static key.
    pub key: [u8; KEY_LEN],
    /// When the pairing happened, in unix seconds.
    pub paired_at: u64,
    /// Free text for a person: the address it paired from, a device name.
    pub label: String,
}

/// This device's identity plus the peers it trusts, optionally backed by a file.
#[derive(Debug)]
pub struct PairingStore {
    path: Option<PathBuf>,
    identity: Identity,
    peers: Vec<PairedPeer>,
}

const HEADER: &str = "# Universal Screens pairing keys. Private: it holds this device's secret key.";

impl PairingStore {
    /// A store that lives only in memory: a fresh identity, nobody trusted, and
    /// nothing written anywhere. For tests, the development host, and as the
    /// fall-back when the real file cannot be used.
    ///
    /// # Errors
    /// If no identity can be generated.
    pub fn ephemeral() -> io::Result<PairingStore> {
        Ok(PairingStore { path: None, identity: Identity::generate()?, peers: Vec::new() })
    }

    /// Open the store at `path`, creating it (and a new identity) if it does
    /// not exist yet.
    ///
    /// # Errors
    /// If the file exists but cannot be read or parsed, or a new one cannot be
    /// written. A corrupt file is reported, never overwritten.
    pub fn open(path: &Path) -> io::Result<PairingStore> {
        match fs::read_to_string(path) {
            Ok(text) => {
                let (identity, peers) = parse(&text)?;
                Ok(PairingStore { path: Some(path.to_path_buf()), identity, peers })
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let store = PairingStore {
                    path: Some(path.to_path_buf()),
                    identity: Identity::generate()?,
                    peers: Vec::new(),
                };
                store.save()?;
                Ok(store)
            }
            Err(e) => Err(e),
        }
    }

    /// Re-read the file, picking up a change made by another part of the app
    /// (the host window's "Forget paired devices" while the accept loop runs).
    /// A store with no file is left as it is.
    ///
    /// # Errors
    /// If the file cannot be read or parsed. The in-memory copy is unchanged.
    pub fn reload(&mut self) -> io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        match fs::read_to_string(path) {
            Ok(text) => {
                let (identity, peers) = parse(&text)?;
                self.identity = identity;
                self.peers = peers;
                Ok(())
            }
            // Deleted underneath us: keep the identity, forget everyone, and
            // write it back so the next reader agrees.
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.peers.clear();
                self.save()
            }
            Err(e) => Err(e),
        }
    }

    /// This device's identity.
    #[must_use]
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// Everyone this device has paired with.
    #[must_use]
    pub fn peers(&self) -> &[PairedPeer] {
        &self.peers
    }

    /// Their public keys, for a reconnecting handshake.
    #[must_use]
    pub fn peer_keys(&self) -> Vec<[u8; KEY_LEN]> {
        self.peers.iter().map(|p| p.key).collect()
    }

    /// Whether `key` belongs to a paired device.
    #[must_use]
    pub fn is_paired(&self, key: &[u8; KEY_LEN]) -> bool {
        self.peers.iter().any(|p| &p.key == key)
    }

    /// Remember `key` as paired (refreshing its label and time if already
    /// known), and save.
    ///
    /// # Errors
    /// If the file cannot be written. The in-memory list is updated regardless.
    pub fn remember(&mut self, key: [u8; KEY_LEN], label: &str) -> io::Result<()> {
        let label: String = label.chars().filter(|c| !c.is_control()).take(120).collect();
        self.peers.retain(|p| p.key != key);
        self.peers.push(PairedPeer { key, paired_at: unix_now(), label });
        self.save()
    }

    /// Forget every paired device (the identity is kept), and save.
    ///
    /// # Errors
    /// If the file cannot be written.
    pub fn forget_all(&mut self) -> io::Result<()> {
        self.peers.clear();
        self.save()
    }

    fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        let mut text = format!(
            "{HEADER}\nversion 1\nidentity {} {}\n",
            to_hex(&self.identity.secret),
            to_hex(&self.identity.public)
        );
        for p in &self.peers {
            text.push_str(&format!("peer {} {} {}\n", to_hex(&p.key), p.paired_at, p.label));
        }
        write_private(path, text.as_bytes())
    }
}

/// Write `bytes` to `path` by way of a temporary file and a rename, so a crash
/// mid-write leaves the old file rather than half of a new one. Owner-only on
/// unix.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

fn parse(text: &str) -> io::Result<(Identity, Vec<PairedPeer>)> {
    let bad = |why: &str| io::Error::new(io::ErrorKind::InvalidData, format!("pairing file: {why}"));
    let mut identity = None;
    let mut peers = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let (tag, rest) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        match tag {
            "version" if rest.trim() == "1" => {}
            "version" => return Err(bad("written by a newer version")),
            "identity" => {
                let mut halves = rest.split(' ');
                let secret = halves.next().and_then(from_hex).ok_or_else(|| bad("bad identity"))?;
                let public = halves.next().and_then(from_hex).ok_or_else(|| bad("bad identity"))?;
                identity = Some(Identity { secret, public });
            }
            "peer" => {
                let mut fields = rest.splitn(3, ' ');
                let key = fields.next().and_then(from_hex).ok_or_else(|| bad("bad peer key"))?;
                let paired_at = fields.next().and_then(|t| t.parse().ok()).unwrap_or(0);
                let label = fields.next().unwrap_or("").to_owned();
                peers.push(PairedPeer { key, paired_at, label });
            }
            // An unknown record from a later version is skipped, not fatal.
            _ => {}
        }
    }
    Ok((identity.ok_or_else(|| bad("no identity"))?, peers))
}

/// Lower-case hex of a key.
#[must_use]
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A 32-byte key from 64 hex digits, or `None`.
#[must_use]
pub fn from_hex(text: &str) -> Option<[u8; KEY_LEN]> {
    let text = text.trim();
    if text.len() != KEY_LEN * 2 {
        return None;
    }
    let mut out = [0u8; KEY_LEN];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// The first eight hex digits of a key — enough for a person to compare two.
#[must_use]
pub fn short(key: &[u8; KEY_LEN]) -> String {
    to_hex(&key[..4])
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "screens-pairing-test-{}-{name}-{}",
            std::process::id(),
            unix_now()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir.join("pairing.txt")
    }

    #[test]
    fn a_new_store_creates_an_identity_and_keeps_it() {
        let path = temp_path("create");
        let first = PairingStore::open(&path).unwrap();
        let again = PairingStore::open(&path).unwrap();
        assert_eq!(first.identity().public(), again.identity().public());
        assert_eq!(first.identity().secret(), again.identity().secret());
        assert!(again.peers().is_empty());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn remembered_peers_survive_a_reopen_and_forget_all_clears_them() {
        let path = temp_path("peers");
        let mut store = PairingStore::open(&path).unwrap();
        store.remember([7u8; 32], "192.168.1.20:51515").unwrap();
        store.remember([7u8; 32], "again").unwrap(); // refresh, not a duplicate
        store.remember([9u8; 32], "").unwrap();

        let mut reopened = PairingStore::open(&path).unwrap();
        assert_eq!(reopened.peers().len(), 2);
        assert!(reopened.is_paired(&[7u8; 32]));
        assert_eq!(reopened.peers()[0].label, "again");

        reopened.forget_all().unwrap();
        store.reload().unwrap();
        assert!(store.peers().is_empty(), "a forget elsewhere reaches a reload");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_corrupt_file_is_an_error_and_is_left_alone() {
        let path = temp_path("corrupt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "identity nonsense\n").unwrap();
        assert!(PairingStore::open(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "identity nonsense\n");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path("mode");
        let _ = PairingStore::open(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn labels_cannot_inject_records() {
        let path = temp_path("inject");
        let mut store = PairingStore::open(&path).unwrap();
        store.remember([1u8; 32], "evil\npeer 0202020202020202020202020202020202020202020202020202020202020202 0 x").unwrap();
        let reopened = PairingStore::open(&path).unwrap();
        assert_eq!(reopened.peers().len(), 1, "a newline in a label must not add a peer");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn hex_round_trips_and_rejects_garbage() {
        let key = [0xABu8; 32];
        assert_eq!(from_hex(&to_hex(&key)), Some(key));
        assert_eq!(from_hex("zz"), None);
        assert_eq!(from_hex(&"g".repeat(64)), None);
    }
}
