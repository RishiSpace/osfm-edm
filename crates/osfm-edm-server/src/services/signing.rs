//! Job signing service — Ed25519 signatures for jobs dispatched to agents.
//!
//! The server holds a persistent Ed25519 signing key (stored at
//! `data/job_signing.key`). Every `DispatchJob` message carries a signature
//! over the canonical job bytes (see `osfm_edm_common::jobs::canonical_job_signing_bytes`).
//! Agents receive the public key at enrollment and refuse to execute unsigned
//! or wrongly-signed jobs — so a network attacker who can impersonate the
//! server still cannot run code on managed devices.

use ed25519_dalek::{Signer, SigningKey};
use std::path::Path;
use uuid::Uuid;

use base64::Engine as _;

const KEY_FILE: &str = "job_signing.key";

/// Errors from the signing service.
#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid signing key on disk: {0}")]
    InvalidKey(String),
}

/// Ed25519 job signer with the public key exposed for enrollment responses.
pub struct JobSigner {
    signing_key: SigningKey,
    /// Base64-encoded 32-byte public key, given to agents during enrollment.
    public_key_b64: String,
}

impl JobSigner {
    /// Load the signing key from disk, or generate and persist a new one.
    pub fn load_or_create(data_dir: &Path) -> Result<Self, SigningError> {
        let key_path = data_dir.join(KEY_FILE);

        let signing_key = if key_path.exists() {
            let raw = std::fs::read(&key_path)?;
            let bytes: [u8; 32] = raw
                .as_slice()
                .try_into()
                .map_err(|_| SigningError::InvalidKey(format!(
                    "expected 32 bytes, found {}",
                    raw.len()
                )))?;
            tracing::info!("Loaded job signing key from {}", key_path.display());
            SigningKey::from_bytes(&bytes)
        } else {
            std::fs::create_dir_all(data_dir)?;
            let mut rng = rand::rngs::OsRng;
            let key = SigningKey::generate(&mut rng);
            std::fs::write(&key_path, key.to_bytes())?;
            restrict_file_permissions(&key_path);
            tracing::info!("Generated new job signing key at {}", key_path.display());
            key
        };

        let public_key_b64 =
            base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes());

        Ok(Self {
            signing_key,
            public_key_b64,
        })
    }

    /// Base64-encoded public key (32 bytes) for agent verification.
    pub fn public_key_b64(&self) -> &str {
        &self.public_key_b64
    }

    /// Sign a job dispatch; returns a base64-encoded Ed25519 signature.
    pub fn sign_job(&self, job_id: &Uuid, payload: &osfm_edm_common::jobs::JobPayload) -> String {
        let msg = osfm_edm_common::jobs::canonical_job_signing_bytes(job_id, payload);
        let sig = self.signing_key.sign(&msg);
        base64::engine::general_purpose::STANDARD.encode(sig.to_bytes())
    }
}

/// Set 0600 permissions on Unix; no-op elsewhere.
fn restrict_file_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!(error = %e, "Failed to restrict signing key permissions");
        }
    }
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    #[test]
    fn sign_and_verify_round_trip() {
        let dir = std::env::temp_dir().join(format!("osfm-sign-test-{}", Uuid::new_v4()));
        let signer = JobSigner::load_or_create(&dir).unwrap();

        let job_id = Uuid::new_v4();
        let payload = osfm_edm_common::jobs::JobPayload::RunScript {
            shell: osfm_edm_common::jobs::ShellType::Bash,
            script: "echo hi".to_string(),
        };

        let sig_b64 = signer.sign_job(&job_id, &payload);

        // Verify exactly as the agent would.
        let pk_bytes = base64::engine::general_purpose::STANDARD
            .decode(signer.public_key_b64())
            .unwrap();
        let vk = VerifyingKey::from_bytes(&pk_bytes.try_into().unwrap()).unwrap();
        let sig_bytes = base64::engine::general_purpose::STANDARD
            .decode(&sig_b64)
            .unwrap();
        let sig = Signature::from_slice(&sig_bytes).unwrap();
        let msg = osfm_edm_common::jobs::canonical_job_signing_bytes(&job_id, &payload);
        assert!(vk.verify(&msg, &sig).is_ok());

        // Tampered payload must fail verification.
        let evil = osfm_edm_common::jobs::JobPayload::RunScript {
            shell: osfm_edm_common::jobs::ShellType::Bash,
            script: "rm -rf /".to_string(),
        };
        let evil_msg = osfm_edm_common::jobs::canonical_job_signing_bytes(&job_id, &evil);
        assert!(vk.verify(&evil_msg, &sig).is_err());

        // Key persistence: loading again yields the same public key.
        let signer2 = JobSigner::load_or_create(&dir).unwrap();
        assert_eq!(signer.public_key_b64(), signer2.public_key_b64());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
