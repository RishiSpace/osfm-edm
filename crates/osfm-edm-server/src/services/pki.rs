//! PKI service — internal CA management, device certificate issuance and validation.

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa,
    KeyPair, KeyUsagePurpose,
};
use sha2::{Digest, Sha256};
use std::path::Path;
use uuid::Uuid;

/// Errors that can occur during PKI operations.
#[derive(Debug, thiserror::Error)]
pub enum PkiError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Certificate generation error: {0}")]
    Rcgen(#[from] rcgen::Error),
    #[error("CA not initialized")]
    CaNotInitialized,
}

/// CA certificate and key pair used for signing device certificates.
pub struct CertificateAuthority {
    /// PEM-encoded CA certificate.
    pub ca_cert_pem: String,
    /// PEM-encoded CA private key.
    pub ca_key_pem: String,
}

impl CertificateAuthority {
    /// Load or create the CA. Checks for existing CA on disk at `data_dir/ca.crt` and `data_dir/ca.key`.
    /// If not found, generates a new self-signed CA and persists it.
    pub fn load_or_create(data_dir: &Path) -> Result<Self, PkiError> {
        let cert_path = data_dir.join("ca.crt");
        let key_path = data_dir.join("ca.key");

        if cert_path.exists() && key_path.exists() {
            tracing::info!("Loading existing CA from {}", data_dir.display());
            let ca_cert_pem = std::fs::read_to_string(&cert_path)?;
            let ca_key_pem = std::fs::read_to_string(&key_path)?;
            return Ok(Self {
                ca_cert_pem,
                ca_key_pem,
            });
        }

        tracing::info!("Generating new CA certificate");
        std::fs::create_dir_all(data_dir)?;

        let mut ca_params = CertificateParams::new(vec!["OSFM-EDM CA".to_string()])?;
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
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
        tracing::info!("CA certificate written to {}", cert_path.display());

        Ok(Self {
            ca_cert_pem,
            ca_key_pem,
        })
    }

    /// Issue a device certificate signed by this CA.
    /// The device_id is embedded in the Subject CN for later extraction.
    pub fn issue_device_cert(&self, device_id: Uuid) -> Result<(String, String), PkiError> {
        let cn = format!("device:{device_id}");
        let mut params = CertificateParams::new(vec![cn.clone()])?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, &cn);
        dn.push(DnType::OrganizationName, "OSFM-EDM Device");
        params.distinguished_name = dn;
        params.is_ca = IsCa::NoCa;

        let device_key_pair = KeyPair::generate()?;

        // Recreate the CA self-signed cert from stored PEM for signing.
        let ca_key_pair = KeyPair::from_pem(&self.ca_key_pem)?;
        let mut ca_params = CertificateParams::new(vec!["OSFM-EDM CA".to_string()])?;
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let mut ca_dn = DistinguishedName::new();
        ca_dn.push(DnType::CommonName, "OSFM-EDM Internal CA");
        ca_dn.push(DnType::OrganizationName, "OSFM-EDM");
        ca_params.distinguished_name = ca_dn;
        let ca_cert = ca_params.self_signed(&ca_key_pair)?;

        let device_cert = params.signed_by(&device_key_pair, &ca_cert, &ca_key_pair)?;

        let cert_pem = device_cert.pem();
        let key_pem = device_key_pair.serialize_pem();

        tracing::info!(device_id = %device_id, "Issued device certificate");
        Ok((cert_pem, key_pem))
    }

    /// Compute the SHA-256 fingerprint of a PEM certificate.
    pub fn fingerprint(cert_pem: &str) -> String {
        let hash = Sha256::digest(cert_pem.as_bytes());
        format!("{:x}", hash)
    }
}
