//! Creates one-time pairing codes and remembers which phones have been paired.

use crate::protocol::RejectReason;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

pub const TOKEN_TTL: Duration = Duration::from_secs(300);
/// How long a paired phone can stay away before it has to scan a new code.
pub const DEVICE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

pub fn random_secret(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

fn hash(secret: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(secret.as_bytes()))
}

fn ct_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[derive(Default)]
pub struct Pairing {
    current: Option<(String, Instant)>,
}

impl Pairing {
    /// Makes a new pairing code, cancels the old one, and returns it with its expiry time.
    pub fn issue(&mut self) -> (String, u64) {
        let token = random_secret(16);
        self.current = Some((token.clone(), Instant::now() + TOKEN_TTL));
        (token, now_ms() + TOKEN_TTL.as_millis() as u64)
    }

    pub fn cancel(&mut self) {
        self.current = None;
    }

    pub fn is_active(&self) -> bool {
        self.current.as_ref().is_some_and(|(_, exp)| Instant::now() < *exp)
    }

    /// Checks a pairing code and uses it up so it cannot work twice.
    pub fn consume(&mut self, token: &str) -> Result<(), RejectReason> {
        let Some((current, expires)) = &self.current else { return Err(RejectReason::InvalidToken) };
        if !ct_eq(current, token) {
            return Err(RejectReason::InvalidToken);
        }
        let expired = Instant::now() >= *expires;
        self.current = None;
        if expired { Err(RejectReason::ExpiredToken) } else { Ok(()) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub user_agent: String,
    pub paired_at: u64,
    pub last_seen: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredDevice {
    #[serde(flatten)]
    device: Device,
    secret_hash: String,
}

pub struct DeviceStore {
    path: PathBuf,
    devices: Vec<StoredDevice>,
}

pub fn clean_name(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
    let name = name.trim();
    if name.is_empty() { "Phone".into() } else { name.into() }
}

fn expired(device: &Device, now: u64) -> bool {
    now.saturating_sub(device.last_seen) > DEVICE_TTL.as_millis() as u64
}

impl DeviceStore {
    pub fn load(path: PathBuf) -> Self {
        let devices = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let mut store = Self { path, devices };
        store.prune(None);
        store
    }

    fn save(&self) {
        let tmp = self.path.with_extension("tmp");
        let ok = serde_json::to_vec_pretty(&self.devices)
            .ok()
            .is_some_and(|json| std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, &self.path).is_ok());
        if !ok {
            tracing::warn!("could not save devices to {}", self.path.display());
        }
    }

    pub fn list(&self) -> Vec<Device> {
        self.devices.iter().map(|d| d.device.clone()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Saves a newly paired phone and returns a private key that only that phone receives.
    pub fn register(&mut self, name: &str, user_agent: &str) -> (Device, String) {
        let secret = random_secret(32);
        let now = now_ms();
        let device = Device {
            id: random_secret(9),
            name: clean_name(name),
            user_agent: user_agent.chars().take(256).collect(),
            paired_at: now,
            last_seen: now,
        };
        self.devices.push(StoredDevice { device: device.clone(), secret_hash: hash(&secret) });
        self.save();
        (device, secret)
    }

    fn find_mut(&mut self, id: &str) -> Option<&mut StoredDevice> {
        self.devices.iter_mut().find(|d| d.device.id == id)
    }

    pub fn verify(&mut self, id: &str, secret: &str) -> Result<Device, RejectReason> {
        let now = now_ms();
        let stored = self.find_mut(id).ok_or(RejectReason::UnknownDevice)?;
        if !ct_eq(&stored.secret_hash, &hash(secret)) || expired(&stored.device, now) {
            return Err(RejectReason::UnknownDevice);
        }
        stored.device.last_seen = now;
        let device = stored.device.clone();
        self.save();
        Ok(device)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> bool {
        let Some(d) = self.find_mut(id) else { return false };
        d.device.name = clean_name(name);
        self.save();
        true
    }

    /// Forgets phones unseen for too long, keeps the connected one fresh, and reports whether any were removed.
    pub fn prune(&mut self, connected: Option<&str>) -> bool {
        let now = now_ms();
        let before = self.devices.len();
        for d in self.devices.iter_mut().filter(|d| Some(d.device.id.as_str()) == connected) {
            d.device.last_seen = now;
        }
        self.devices.retain(|d| !expired(&d.device, now));
        let removed = self.devices.len() != before;
        if removed || connected.is_some() {
            self.save();
        }
        removed
    }

    pub fn forget(&mut self, id: &str) -> bool {
        let before = self.devices.len();
        self.devices.retain(|d| d.device.id != id);
        self.save();
        self.devices.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_single_use() {
        let mut p = Pairing::default();
        let (token, _) = p.issue();
        assert_eq!(p.consume("wrong"), Err(RejectReason::InvalidToken));
        assert_eq!(p.consume(&token), Ok(()));
        assert_eq!(p.consume(&token), Err(RejectReason::InvalidToken));
    }

    #[test]
    fn new_token_invalidates_old() {
        let mut p = Pairing::default();
        let (old, _) = p.issue();
        let (new, _) = p.issue();
        assert_eq!(p.consume(&old), Err(RejectReason::InvalidToken));
        assert_eq!(p.consume(&new), Ok(()));
    }

    #[test]
    fn expired_token_is_rejected() {
        let mut p = Pairing::default();
        let (token, _) = p.issue();
        p.current.as_mut().unwrap().1 = Instant::now() - Duration::from_secs(1);
        assert_eq!(p.consume(&token), Err(RejectReason::ExpiredToken));
    }

    #[test]
    fn device_store_roundtrip() {
        let dir = std::env::temp_dir().join(format!("oc-test-{}", random_secret(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("devices.json");
        let mut store = DeviceStore::load(path.clone());
        let (device, secret) = store.register("Pixel\u{7}  ", "UA");
        assert_eq!(device.name, "Pixel");
        assert!(store.verify(&device.id, "nope").is_err());

        let mut reloaded = DeviceStore::load(path.clone());
        assert_eq!(reloaded.verify(&device.id, &secret).unwrap().id, device.id);
        assert!(!std::fs::read_to_string(&path).unwrap().contains(&secret));
        assert!(reloaded.rename(&device.id, "Work phone"));
        assert!(reloaded.forget(&device.id));
        assert_eq!(reloaded.verify(&device.id, &secret), Err(RejectReason::UnknownDevice));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stale_devices_are_forgotten() {
        let dir = std::env::temp_dir().join(format!("oc-test-{}", random_secret(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = DeviceStore::load(dir.join("devices.json"));
        let (old, old_secret) = store.register("Old", "UA");
        let (live, _) = store.register("Live", "UA");
        let stale = now_ms() - DEVICE_TTL.as_millis() as u64 - 1;
        store.devices.iter_mut().for_each(|d| d.device.last_seen = stale);

        assert_eq!(store.verify(&old.id, &old_secret), Err(RejectReason::UnknownDevice));
        assert!(store.prune(Some(&live.id)));
        assert_eq!(store.list().iter().map(|d| d.id.clone()).collect::<Vec<_>>(), vec![live.id]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
