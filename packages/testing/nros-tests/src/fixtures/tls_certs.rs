//! TLS certificate generation for integration tests
//!
//! Generates self-signed EC certificates using the `openssl` CLI tool.
//! Certificates are written to a temporary directory that persists for
//! the lifetime of the returned [`TlsCerts`] struct.

use std::path::{Path, PathBuf};

/// Self-signed TLS certificate and private key for testing.
///
/// The certificate and key files are written to a temporary directory
/// that is cleaned up when this struct is dropped.
pub struct TlsCerts {
    _dir: tempfile::TempDir,
    cert_path: PathBuf,
    key_path: PathBuf,
}

impl TlsCerts {
    /// Generate a self-signed EC certificate for testing.
    ///
    /// Uses `openssl req` with prime256v1 (P-256) curve.
    /// The certificate is valid for 1 day with CN=localhost.
    pub fn generate() -> Result<Self, String> {
        let dir =
            tempfile::TempDir::new().map_err(|e| format!("Failed to create temp dir: {}", e))?;

        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");

        let output = std::process::Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "ec",
                "-pkeyopt",
                "ec_paramgen_curve:prime256v1",
                "-keyout",
            ])
            .arg(&key_path)
            .arg("-out")
            .arg(&cert_path)
            .args(["-days", "1", "-nodes", "-subj", "/CN=localhost"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .map_err(|e| format!("Failed to run openssl: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "openssl failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(Self {
            _dir: dir,
            cert_path,
            key_path,
        })
    }

    /// Path to the PEM certificate file.
    pub fn cert_path(&self) -> &Path {
        &self.cert_path
    }

    /// Path to the PEM private key file.
    pub fn key_path(&self) -> &Path {
        &self.key_path
    }
}

/// Check if `openssl` CLI is available.
pub fn is_openssl_available() -> bool {
    std::process::Command::new("openssl")
        .arg("version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openssl_available() {
        // Just check detection works
        let available = is_openssl_available();
        eprintln!("openssl available: {}", available);
    }

    #[test]
    fn test_generate_certs() {
        if !is_openssl_available() {
            eprintln!("Skipping: openssl not available");
            return;
        }

        let certs = TlsCerts::generate().expect("Failed to generate certs");
        assert!(certs.cert_path().exists());
        assert!(certs.key_path().exists());

        // Verify cert is valid PEM
        let cert_content = std::fs::read_to_string(certs.cert_path()).unwrap();
        assert!(cert_content.contains("BEGIN CERTIFICATE"));

        let key_content = std::fs::read_to_string(certs.key_path()).unwrap();
        assert!(key_content.contains("BEGIN"));
    }
}
