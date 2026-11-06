use std::{
    collections::HashSet,
    fmt,
    net::{IpAddr, SocketAddr},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use ipnet::IpNet;
use serde::Serialize;
use thiserror::Error;
use zeroize::Zeroizing;

pub const MAX_CONFIG_BYTES: usize = 65_536;

pub struct SecretKey(Zeroizing<[u8; 32]>);

impl SecretKey {
    fn parse(value: &str) -> Result<Self, &'static str> {
        let mut bytes = Zeroizing::new([0; 32]);
        let length = STANDARD
            .decode_slice(value, bytes.as_mut())
            .map_err(|_| "key must be a base64-encoded 32-byte value")?;
        if length != 32 || bytes.iter().all(|byte| *byte == 0) {
            return Err("key must be a nonzero 32-byte value");
        }
        Ok(Self(bytes))
    }

    /// Only tunnel backends and protected storage should access key material.
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Address(SocketAddr),
    Host { name: String, port: u16 },
}

impl Endpoint {
    fn parse(value: &str) -> Result<Self, &'static str> {
        if let Ok(address) = value.parse::<SocketAddr>() {
            if address.port() == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
                return Err("endpoint address or port is invalid");
            }
            return Ok(Self::Address(address));
        }
        let (name, port) = value
            .rsplit_once(':')
            .ok_or("endpoint must contain a host and port")?;
        let port = port
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0)
            .ok_or("invalid endpoint port")?;
        if name.is_empty()
            || name.len() > 253
            || !name.is_ascii()
            || name.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
        {
            return Err("invalid endpoint hostname; IPv6 literals require brackets");
        }
        Ok(Self::Host {
            name: name.into(),
            port,
        })
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(address) => address.fmt(f),
            Self::Host { name, port } => write!(f, "{name}:{port}"),
        }
    }
}

#[derive(Debug)]
pub struct Peer {
    pub public_key: SecretKey,
    pub preshared_key: Option<SecretKey>,
    pub endpoint: Endpoint,
    pub allowed_ips: Vec<IpNet>,
    pub persistent_keepalive: Option<u16>,
}

#[derive(Debug)]
pub struct Profile {
    pub private_key: SecretKey,
    pub addresses: Vec<IpNet>,
    pub dns: Vec<IpAddr>,
    pub mtu: u16,
    pub listen_port: u16,
    pub peer: Peer,
}

#[derive(Debug, Serialize)]
pub struct ProfileSummary {
    pub addresses: Vec<String>,
    pub dns: Vec<IpAddr>,
    pub mtu: u16,
    pub listen_port: u16,
    pub endpoint: String,
    pub endpoint_needs_bootstrap_dns: bool,
    pub allowed_ips: Vec<String>,
    pub has_preshared_key: bool,
    pub full_tunnel_ipv4: bool,
    pub full_tunnel_ipv6: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("configuration error at line {line}: {message}")]
pub struct ConfigError {
    pub line: usize,
    pub message: &'static str,
}

impl Profile {
    pub fn parse(input: &str) -> Result<Self, ConfigError> {
        if input.len() > MAX_CONFIG_BYTES {
            return Err(error(0, "configuration exceeds 64 KiB"));
        }
        let mut section = "";
        let mut seen = HashSet::new();
        let (mut private_key, mut public_key, mut preshared_key) = (None, None, None);
        let (mut addresses, mut dns, mut allowed_ips) = (None, None, None);
        let (mut endpoint, mut keepalive) = (None, None);
        let (mut mtu, mut listen_port) = (1280, 0);
        let (mut interface_seen, mut peer_seen) = (false, false);

        for (index, raw) in input.trim_start_matches('\u{feff}').lines().enumerate() {
            let line = index + 1;
            let text = raw.split('#').next().unwrap_or_default().trim();
            if text.is_empty() {
                continue;
            }
            if text.starts_with('[') {
                match text {
                    "[Interface]" if !interface_seen && !peer_seen => {
                        interface_seen = true;
                        section = "interface";
                    }
                    "[Peer]" if interface_seen && !peer_seen => {
                        peer_seen = true;
                        section = "peer";
                    }
                    _ => {
                        return Err(error(
                            line,
                            "expected one Interface followed by one Peer section",
                        ))
                    }
                }
                continue;
            }
            let (key, value) = text
                .split_once('=')
                .ok_or(error(line, "expected key = value"))?;
            let (key, value) = (key.trim(), value.trim());
            if !seen.insert((section.to_owned(), key.to_owned())) {
                return Err(error(line, "duplicate field"));
            }
            let result: Result<(), &'static str> =
                (|| {
                    match (section, key) {
                        ("interface", "PrivateKey") => private_key = Some(SecretKey::parse(value)?),
                        ("interface", "Address") => addresses = Some(parse_list::<IpNet>(value)?),
                        ("interface", "DNS") => dns = Some(parse_list::<IpAddr>(value)?),
                        ("interface", "MTU") => mtu = value.parse().map_err(|_| "invalid MTU")?,
                        ("interface", "ListenPort") => {
                            listen_port = value.parse().map_err(|_| "invalid listen port")?
                        }
                        ("peer", "PublicKey") => public_key = Some(SecretKey::parse(value)?),
                        ("peer", "PresharedKey") => preshared_key = Some(SecretKey::parse(value)?),
                        ("peer", "Endpoint") => endpoint = Some(Endpoint::parse(value)?),
                        ("peer", "AllowedIPs") => allowed_ips = Some(parse_list::<IpNet>(value)?),
                        ("peer", "PersistentKeepalive") => {
                            keepalive = Some(value.parse().map_err(|_| "invalid keepalive")?)
                        }
                        (_, "PreUp" | "PostUp" | "PreDown" | "PostDown") => return Err(
                            "configuration scripts are not supported and will never be executed",
                        ),
                        _ => return Err("unsupported field or field outside its section"),
                    }
                    Ok(())
                })();
            result.map_err(|message| error(line, message))?;
        }
        let addresses: Vec<IpNet> = addresses.ok_or(error(0, "Address is required"))?;
        if addresses.len() > 2
            || addresses.iter().filter(|a| a.addr().is_ipv4()).count() > 1
            || addresses.iter().filter(|a| a.addr().is_ipv6()).count() > 1
            || addresses
                .iter()
                .any(|a| a.addr().is_unspecified() || a.addr().is_multicast())
        {
            return Err(error(0, "use at most one usable address per IP family"));
        }
        if !(1280..=9000).contains(&mtu) {
            return Err(error(0, "MTU must be between 1280 and 9000"));
        }
        let allowed_ips: Vec<IpNet> = allowed_ips.ok_or(error(0, "AllowedIPs is required"))?;
        if allowed_ips.len() > 256 {
            return Err(error(0, "too many AllowedIPs entries"));
        }
        let dns: Vec<IpAddr> = dns.unwrap_or_default();
        if dns.len() > 2
            || dns.iter().any(|ip| {
                ip.is_unspecified()
                    || ip.is_multicast()
                    || !allowed_ips.iter().any(|net| net.contains(ip))
                    || !addresses.iter().any(|a| a.addr().is_ipv4() == ip.is_ipv4())
            })
        {
            return Err(error(0, "use at most two DNS server IPs reachable through AllowedIPs and an assigned address family"));
        }
        Ok(Self {
            private_key: private_key.ok_or(error(0, "PrivateKey is required"))?,
            addresses,
            dns,
            mtu,
            listen_port,
            peer: Peer {
                public_key: public_key.ok_or(error(0, "PublicKey is required"))?,
                preshared_key,
                endpoint: endpoint.ok_or(error(0, "Endpoint is required"))?,
                allowed_ips,
                persistent_keepalive: keepalive.filter(|value| *value != 0),
            },
        })
    }

    pub fn allows(&self, address: IpAddr) -> bool {
        self.peer
            .allowed_ips
            .iter()
            .any(|net| net.contains(&address))
            && self
                .addresses
                .iter()
                .any(|local| local.addr().is_ipv4() == address.is_ipv4())
    }

    pub fn summary(&self) -> ProfileSummary {
        let networks = IpNet::aggregate(&self.peer.allowed_ips);
        ProfileSummary {
            addresses: self.addresses.iter().map(ToString::to_string).collect(),
            dns: self.dns.clone(),
            mtu: self.mtu,
            listen_port: self.listen_port,
            endpoint: self.peer.endpoint.to_string(),
            endpoint_needs_bootstrap_dns: matches!(self.peer.endpoint, Endpoint::Host { .. }),
            allowed_ips: self
                .peer
                .allowed_ips
                .iter()
                .map(ToString::to_string)
                .collect(),
            has_preshared_key: self.peer.preshared_key.is_some(),
            full_tunnel_ipv4: self
                .addresses
                .iter()
                .any(|address| address.addr().is_ipv4())
                && networks
                    .iter()
                    .any(|n| n.addr().is_ipv4() && n.prefix_len() == 0),
            full_tunnel_ipv6: self
                .addresses
                .iter()
                .any(|address| address.addr().is_ipv6())
                && networks
                    .iter()
                    .any(|n| n.addr().is_ipv6() && n.prefix_len() == 0),
        }
    }
}

fn parse_list<T: std::str::FromStr>(value: &str) -> Result<Vec<T>, &'static str> {
    value
        .split(',')
        .map(|part| {
            part.trim()
                .parse()
                .map_err(|_| "invalid address or network list")
        })
        .collect()
}

fn error(line: usize, message: &'static str) -> ConfigError {
    ConfigError { line, message }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CONFIG: &str = include_str!("../../../tests/fixtures/example.conf");

    #[test]
    fn parses_profile_and_retains_host_address() {
        let profile = Profile::parse(CONFIG).unwrap();
        assert_eq!(profile.addresses[0].addr().to_string(), "10.77.0.2");
        assert!(profile.summary().full_tunnel_ipv4);
        assert!(profile.allows("1.1.1.1".parse().unwrap()));
        assert!(!profile.allows("::1".parse().unwrap()));
    }

    #[test]
    fn secrets_are_redacted_even_in_errors() {
        let profile = Profile::parse(CONFIG).unwrap();
        let key = STANDARD.encode(profile.private_key.expose());
        assert!(!format!("{profile:?}").contains(&key));
        let invalid = CONFIG.replace(&key, "secret-not-a-key");
        assert!(!Profile::parse(&invalid)
            .unwrap_err()
            .to_string()
            .contains("secret-not-a-key"));
    }

    #[test]
    fn rejects_scripts_duplicates_and_second_peer() {
        for extra in ["\nPostUp = dangerous", "\nPublicKey = secret", "\n[Peer]\n"] {
            assert!(Profile::parse(&format!("{CONFIG}{extra}")).is_err());
        }
    }

    #[test]
    fn rejects_unreachable_dns_and_invalid_values() {
        for config in [
            CONFIG.replace("0.0.0.0/0", "10.77.0.0/24"),
            CONFIG.replace("51820", "0"),
            CONFIG.replace("1280", "100"),
            CONFIG.replace("10.77.0.2/32", "10.77.0.2/32, 10.77.0.3/32"),
            CONFIG.replace("Address = 10.77.0.2/32", "Unknown = value"),
        ] {
            assert!(Profile::parse(&config).is_err());
        }
    }

    #[test]
    fn rejects_large_input_and_supports_bom_crlf() {
        assert!(Profile::parse(&" ".repeat(MAX_CONFIG_BYTES + 1)).is_err());
        assert!(Profile::parse(&format!("\u{feff}{}", CONFIG.replace('\n', "\r\n"))).is_ok());
    }

    #[test]
    fn endpoint_ipv6_requires_brackets() {
        assert!(Endpoint::parse("[::1]:51820").is_ok());
        assert!(Endpoint::parse("2001:db8::1:51820").is_err());
        assert!(Endpoint::parse("vpn.example.com:51820").is_ok());
    }

    #[test]
    fn recognizes_two_half_routes_as_full_ipv4_coverage() {
        let profile =
            Profile::parse(&CONFIG.replace("0.0.0.0/0", "0.0.0.0/1, 128.0.0.0/1, ::/0")).unwrap();
        assert!(profile.summary().full_tunnel_ipv4);
        assert!(!profile.summary().full_tunnel_ipv6);
    }
}
