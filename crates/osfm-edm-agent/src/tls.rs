//! Trust-anchor helpers: pin the server CA (DER SHA-256) and build rustls config.

use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::CertificateDer;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub fn fingerprint_pem(pem: &str) -> Option<String> {
    let mut bytes = pem.as_bytes();
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut bytes)
        .filter_map(|c| c.ok())
        .collect();
    certs.first().map(|c| format!("{:x}", Sha256::digest(c.as_ref())))
}

pub fn normalize_fingerprint(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_lowercase()
}

pub fn fingerprints_match(pem: &str, expected: &str) -> bool {
    match fingerprint_pem(pem) {
        Some(got) => got == normalize_fingerprint(expected),
        None => false,
    }
}

pub fn rustls_config_from_ca_pem(pem: &str) -> Result<Arc<ClientConfig>, String> {
    let mut roots = RootCertStore::empty();
    let mut bytes = pem.as_bytes();
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("parse CA: {e}"))?;
    for c in certs {
        roots
            .add(c)
            .map_err(|e| format!("add CA: {e}"))?;
    }
    if roots.is_empty() {
        return Err("CA PEM contained no certificates".into());
    }
    Ok(Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_colons() {
        assert_eq!(normalize_fingerprint("AB:cd:12"), "abcd12");
    }
}
