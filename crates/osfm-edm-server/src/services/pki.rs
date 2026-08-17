//! Internal CA, device certs, and the TLS server certificate.

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, SanType,
};
use sha2::{Digest, Sha256};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PkiError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Certificate generation error: {0}")]
    Rcgen(#[from] rcgen::Error),
}

pub struct CertificateAuthority {
    pub ca_cert_pem: String,
    pub ca_key_pem: String,
}

impl CertificateAuthority {
    pub fn load_or_create(data_dir: &Path) -> Result<Self, PkiError> {
        let cert_path = data_dir.join("ca.crt");
        let key_path = data_dir.join("ca.key");

        if cert_path.exists() && key_path.exists() {
            tracing::info!("Loading existing CA from {}", data_dir.display());
            return Ok(Self {
                ca_cert_pem: std::fs::read_to_string(&cert_path)?,
                ca_key_pem: std::fs::read_to_string(&key_path)?,
            });
        }

        tracing::info!("Generating new CA certificate");
        std::fs::create_dir_all(data_dir)?;

        let mut ca_params = CertificateParams::new(Vec::<String>::new())?;
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "OSFM-EDM Internal CA");
        dn.push(DnType::OrganizationName, "OSFM-EDM");
        ca_params.distinguished_name = dn;

        let ca_key_pair = KeyPair::generate()?;
        let ca_cert = ca_params.self_signed(&ca_key_pair)?;
        let ca_cert_pem = ca_cert.pem();
        let ca_key_pem = ca_key_pair.serialize_pem();

        std::fs::write(&cert_path, &ca_cert_pem)?;
        std::fs::write(&key_path, &ca_key_pem)?;
        restrict_key_permissions(&key_path);

        Ok(Self {
            ca_cert_pem,
            ca_key_pem,
        })
    }

    fn issuer(&self) -> Result<Issuer<'_, KeyPair>, PkiError> {
        let key = KeyPair::from_pem(&self.ca_key_pem)?;
        Ok(Issuer::from_ca_cert_pem(&self.ca_cert_pem, key)?)
    }

    pub fn issue_device_cert(&self, device_id: Uuid) -> Result<(String, String), PkiError> {
        let cn = format!("device:{device_id}");
        let mut params = CertificateParams::new(vec![cn.clone()])?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, &cn);
        dn.push(DnType::OrganizationName, "OSFM-EDM Device");
        params.distinguished_name = dn;
        params.is_ca = IsCa::NoCa;

        let device_key = KeyPair::generate()?;
        let issuer = self.issuer()?;
        let device_cert = params.signed_by(&device_key, &issuer)?;
        Ok((device_cert.pem(), device_key.serialize_pem()))
    }

    /// Server cert used for rustls. SANs cover the configured public host plus localhost.
    pub fn issue_server_cert(&self, hostnames: &[String]) -> Result<(String, String), PkiError> {
        let mut params = CertificateParams::new(Vec::<String>::new())?;
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        let mut dn = DistinguishedName::new();
        dn.push(
            DnType::CommonName,
            hostnames.first().map(String::as_str).unwrap_or("localhost"),
        );
        params.distinguished_name = dn;
        for name in hostnames {
            if let Ok(ip) = name.parse::<std::net::IpAddr>() {
                params.subject_alt_names.push(SanType::IpAddress(ip));
            } else if let Ok(dns) = name.as_str().try_into() {
                params.subject_alt_names.push(SanType::DnsName(dns));
            }
        }

        let key = KeyPair::generate()?;
        let issuer = self.issuer()?;
        let cert = params.signed_by(&key, &issuer)?;
        Ok((cert.pem(), key.serialize_pem()))
    }

    /// SHA-256 of the CA certificate DER (hex). Agents pin this.
    pub fn ca_fingerprint_sha256(&self) -> Result<String, PkiError> {
        Ok(fingerprint_der_pem(&self.ca_cert_pem))
    }
}

pub fn fingerprint_der_pem(pem: &str) -> String {
    let mut bytes = pem.as_bytes();
    let certs: Vec<_> = rustls_pemfile::certs(&mut bytes).filter_map(|c| c.ok()).collect();
    let der = certs.first().map(|c| c.as_ref()).unwrap_or(pem.as_bytes());
    format!("{:x}", Sha256::digest(der))
}

pub fn normalize_fingerprint(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_lowercase()
}

pub fn load_or_create_server_material(
    data_dir: &Path,
    ca: &CertificateAuthority,
    hosts: &[String],
) -> Result<(String, String), PkiError> {
    let cert_path = data_dir.join("server.crt");
    let key_path = data_dir.join("server.key");
    if cert_path.exists() && key_path.exists() {
        return Ok((
            std::fs::read_to_string(&cert_path)?,
            std::fs::read_to_string(&key_path)?,
        ));
    }
    let (cert, key) = ca.issue_server_cert(hosts)?;
    std::fs::write(&cert_path, &cert)?;
    std::fs::write(&key_path, &key)?;
    restrict_key_permissions(&key_path);
    tracing::info!("Issued TLS server certificate for {hosts:?}");
    Ok((cert, key))
}

fn restrict_key_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!(error = %e, path = %path.display(), "Failed to restrict key permissions");
        }
    }
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_hex_and_stable() {
        let dir = std::env::temp_dir().join(format!("osfm-pki-{}", Uuid::new_v4()));
        let ca = CertificateAuthority::load_or_create(&dir).unwrap();
        let a = ca.ca_fingerprint_sha256().unwrap();
        let b = ca.ca_fingerprint_sha256().unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert_eq!(normalize_fingerprint("AB:cd"), "abcd");
        let _ = std::fs::remove_dir_all(dir);
    }
}
