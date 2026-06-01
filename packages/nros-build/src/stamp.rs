//! SHA-256 input digest + on-disk stamp for incremental skip.
//!
//! The stamp file lives at `$OUT_DIR/nros-gen/.stamp` and contains the
//! lowercase hex-encoded digest of:
//!   - the contents of every input file (package.xml, .msg, .srv, .action)
//!   - the args list passed to `nros codegen`
//!
//! A second invocation with the same inputs reads the stamp, compares the
//! recomputed digest, and skips codegen on match.

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One input file contributing to the digest.
#[derive(Debug, Clone)]
pub struct StampInput {
    pub path: PathBuf,
}

/// Compute the input digest from file contents + args.
///
/// Files are hashed in the order given (caller pre-sorts for determinism).
pub fn compute_digest(inputs: &[StampInput], args: &[String]) -> io::Result<String> {
    let mut h = Sha256::new();
    for inp in inputs {
        h.update(b"FILE\0");
        h.update(inp.path.to_string_lossy().as_bytes());
        h.update(b"\0");
        let bytes = fs::read(&inp.path)?;
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(&bytes);
    }
    for a in args {
        h.update(b"ARG\0");
        h.update(a.as_bytes());
        h.update(b"\0");
    }
    Ok(hex_encode(&h.finalize()))
}

/// Load a previously written stamp; `Ok(None)` if absent.
pub fn load_stamp(stamp_path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(stamp_path) {
        Ok(s) => Ok(Some(s.trim().to_string())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Atomically write the stamp.
pub fn save_stamp(stamp_path: &Path, digest: &str) -> io::Result<()> {
    if let Some(parent) = stamp_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = stamp_path.with_extension("stamp.tmp");
    fs::write(&tmp, digest)?;
    fs::rename(&tmp, stamp_path)?;
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}
