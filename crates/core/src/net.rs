//! Finds this computer's local network addresses and makes the security certificate phones need for camera access.

use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::path::Path;

fn is_private(ip: &Ipv4Addr) -> bool {
    ip.is_private() && !ip.is_loopback() && !ip.is_link_local()
}

/// Lists this computer's addresses on the local network.
pub fn lan_addresses() -> Vec<Ipv4Addr> {
    let mut ips: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.ip() {
            IpAddr::V4(ip) if is_private(&ip) => Some(ip),
            _ => None,
        })
        .collect();
    ips.sort();
    ips.dedup();
    ips
}

/// Picks the address this computer normally uses to reach other machines, without sending anything.
pub fn primary_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if is_private(&ip) => Some(ip),
        _ => lan_addresses().into_iter().next(),
    }
}

pub struct Tls {
    pub cert_pem: String,
    pub key_pem: String,
}

/// Reuses the saved certificate, or makes a new one when it does not cover every current address.
pub fn load_or_create_cert(dir: &Path, addresses: &[Ipv4Addr]) -> crate::Result<Tls> {
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    let sans_path = dir.join("cert-sans.json");
    let wanted: Vec<String> = addresses.iter().map(|a| a.to_string()).collect();
    if let (Ok(cert_pem), Ok(key_pem), Ok(sans)) =
        (std::fs::read_to_string(&cert_path), std::fs::read_to_string(&key_path), std::fs::read(&sans_path))
    {
        let covered: Vec<String> = serde_json::from_slice(&sans).unwrap_or_default();
        if wanted.iter().all(|a| covered.contains(a)) {
            return Ok(Tls { cert_pem, key_pem });
        }
    }
    let mut names = vec!["localhost".to_string()];
    names.extend_from_slice(&wanted);
    let mut params = rcgen::CertificateParams::new(names)?;
    params.distinguished_name.push(rcgen::DnType::CommonName, "OpenCamera");
    params.not_before = rcgen::date_time_ymd(2024, 1, 1);
    params.not_after = rcgen::date_time_ymd(2034, 1, 1);
    let key = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    let tls = Tls { cert_pem: cert.pem(), key_pem: key.serialize_pem() };
    std::fs::create_dir_all(dir)?;
    std::fs::write(&cert_path, &tls.cert_pem)?;
    std::fs::write(&key_path, &tls.key_pem)?;
    std::fs::write(&sans_path, serde_json::to_vec(&wanted)?)?;
    Ok(tls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cert_is_reused_until_addresses_change() {
        let dir = std::env::temp_dir().join(format!("oc-tls-{}", crate::pairing::random_secret(6)));
        let a = load_or_create_cert(&dir, &["192.168.1.5".parse().unwrap()]).unwrap();
        let b = load_or_create_cert(&dir, &["192.168.1.5".parse().unwrap()]).unwrap();
        assert_eq!(a.cert_pem, b.cert_pem);
        let c = load_or_create_cert(&dir, &["10.0.0.7".parse().unwrap()]).unwrap();
        assert_ne!(a.cert_pem, c.cert_pem);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
