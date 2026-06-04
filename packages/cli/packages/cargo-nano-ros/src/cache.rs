//! Caching system for ROS 2 bindings generation
//!
//! This module provides SHA256-based caching to avoid regenerating bindings
//! when interface files haven't changed.

use eyre::{Result, WrapErr};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

/// Cache file name
pub const CACHE_FILE_NAME: &str = ".ros2_bindgen_cache";

/// Cache entry for a single package
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheEntry {
    /// Package name
    pub package_name: String,
    /// SHA256 checksum of all interface files
    pub checksum: String,
    /// ROS distro (e.g., "humble", "iron", "jazzy")
    pub ros_distro: Option<String>,
    /// Package version
    pub package_version: Option<String>,
    /// Timestamp of last generation (Unix timestamp)
    pub timestamp: u64,
    /// Output directory where bindings were generated
    pub output_dir: PathBuf,
}

/// Cache for ROS 2 bindings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cache {
    /// Cache format version
    pub version: u32,
    /// Map of package name to cache entry
    pub entries: HashMap<String, CacheEntry>,
}

impl Cache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Cache {
            version: 1,
            entries: HashMap::new(),
        }
    }

    /// Load cache from file
    pub fn load(cache_file: &Path) -> Result<Self> {
        if !cache_file.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(cache_file)
            .wrap_err_with(|| format!("Failed to read cache file: {}", cache_file.display()))?;

        let cache: Cache =
            serde_json::from_str(&content).wrap_err("Failed to parse cache file (invalid JSON)")?;

        Ok(cache)
    }

    /// Save cache to file
    pub fn save(&self, cache_file: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self).wrap_err("Failed to serialize cache")?;

        fs::write(cache_file, content)
            .wrap_err_with(|| format!("Failed to write cache file: {}", cache_file.display()))?;

        Ok(())
    }

    /// Get cache entry for a package
    pub fn get(&self, package_name: &str) -> Option<&CacheEntry> {
        self.entries.get(package_name)
    }

    /// Insert or update cache entry
    pub fn insert(&mut self, entry: CacheEntry) {
        self.entries.insert(entry.package_name.clone(), entry);
    }

    /// Remove cache entry
    pub fn remove(&mut self, package_name: &str) -> Option<CacheEntry> {
        self.entries.remove(package_name)
    }

    /// Check if cache entry is valid (checksum matches, output exists)
    pub fn is_valid(&self, package_name: &str, current_checksum: &str) -> bool {
        if let Some(entry) = self.get(package_name) {
            // Check checksum match
            if entry.checksum != current_checksum {
                return false;
            }

            // Check output directory exists
            if !entry.output_dir.exists() {
                return false;
            }

            // TODO: Check ROS_DISTRO if needed
            // TODO: Check package version if needed

            true
        } else {
            false
        }
    }

    /// Get number of cached packages
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get iterator over cache entries
    pub fn entries(&self) -> impl Iterator<Item = &CacheEntry> {
        self.entries.values()
    }
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate SHA256 checksum of a directory of interface files
pub fn calculate_package_checksum(package_share_dir: &Path) -> Result<String> {
    let mut all_content = Vec::new();

    // Collect all .msg files
    if let Ok(msg_dir) = fs::read_dir(package_share_dir.join("msg")) {
        for entry in msg_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("msg") {
                let content = fs::read(&path)
                    .wrap_err_with(|| format!("Failed to read {}", path.display()))?;
                all_content.extend_from_slice(&content);
            }
        }
    }

    // Collect all .srv files
    if let Ok(srv_dir) = fs::read_dir(package_share_dir.join("srv")) {
        for entry in srv_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("srv") {
                let content = fs::read(&path)
                    .wrap_err_with(|| format!("Failed to read {}", path.display()))?;
                all_content.extend_from_slice(&content);
            }
        }
    }

    // Collect all .action files
    if let Ok(action_dir) = fs::read_dir(package_share_dir.join("action")) {
        for entry in action_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("action") {
                let content = fs::read(&path)
                    .wrap_err_with(|| format!("Failed to read {}", path.display()))?;
                all_content.extend_from_slice(&content);
            }
        }
    }

    // Calculate SHA256 of all content
    let mut hasher = Sha256::new();
    hasher.update(&all_content);
    let result = hasher.finalize();

    // Convert to hex string
    Ok(format!("{:x}", result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_cache_new() {
        let cache = Cache::new();
        assert_eq!(cache.version, 1);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_insert_get() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut cache = Cache::new();
        let entry = CacheEntry {
            package_name: "test_msgs".to_string(),
            checksum: "abc123".to_string(),
            ros_distro: Some("humble".to_string()),
            package_version: Some("1.0.0".to_string()),
            timestamp: 1234567890,
            output_dir: temp_dir.path().to_path_buf(),
        };

        cache.insert(entry.clone());
        assert_eq!(cache.len(), 1);

        let retrieved = cache.get("test_msgs").unwrap();
        assert_eq!(retrieved.checksum, "abc123");
    }

    #[test]
    fn test_cache_remove() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut cache = Cache::new();
        let entry = CacheEntry {
            package_name: "test_msgs".to_string(),
            checksum: "abc123".to_string(),
            ros_distro: None,
            package_version: None,
            timestamp: 1234567890,
            output_dir: temp_dir.path().to_path_buf(),
        };

        cache.insert(entry);
        assert_eq!(cache.len(), 1);

        cache.remove("test_msgs");
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_load_save() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_file = temp_dir.path().join(CACHE_FILE_NAME);
        let output_dir = temp_dir.path().join("output");

        // Create and save cache
        let mut cache = Cache::new();
        cache.insert(CacheEntry {
            package_name: "test_msgs".to_string(),
            checksum: "def456".to_string(),
            ros_distro: Some("iron".to_string()),
            package_version: Some("2.0.0".to_string()),
            timestamp: 9876543210,
            output_dir,
        });

        cache.save(&cache_file).unwrap();
        assert!(cache_file.exists());

        // Load and verify
        let loaded = Cache::load(&cache_file).unwrap();
        assert_eq!(loaded.len(), 1);
        let entry = loaded.get("test_msgs").unwrap();
        assert_eq!(entry.checksum, "def456");
        assert_eq!(entry.ros_distro, Some("iron".to_string()));
    }

    #[test]
    fn test_cache_load_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_file = temp_dir.path().join("nonexistent.json");

        let cache = Cache::load(&cache_file).unwrap();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_calculate_checksum() {
        let temp_dir = tempfile::tempdir().unwrap();
        let share_dir = temp_dir.path().join("test_pkg");

        // Create test interface files
        let msg_dir = share_dir.join("msg");
        fs::create_dir_all(&msg_dir).unwrap();
        fs::write(msg_dir.join("Point.msg"), "float64 x\nfloat64 y\n").unwrap();

        let checksum = calculate_package_checksum(&share_dir).unwrap();
        assert!(!checksum.is_empty());
        assert_eq!(checksum.len(), 64); // SHA256 is 64 hex chars

        // Same content should produce same checksum
        let checksum2 = calculate_package_checksum(&share_dir).unwrap();
        assert_eq!(checksum, checksum2);
    }

    #[test]
    fn test_checksum_changes_with_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let share_dir = temp_dir.path().join("test_pkg");
        let msg_dir = share_dir.join("msg");
        fs::create_dir_all(&msg_dir).unwrap();

        // Initial content
        fs::write(msg_dir.join("Point.msg"), "float64 x\n").unwrap();
        let checksum1 = calculate_package_checksum(&share_dir).unwrap();

        // Modified content
        fs::write(msg_dir.join("Point.msg"), "float64 x\nfloat64 y\n").unwrap();
        let checksum2 = calculate_package_checksum(&share_dir).unwrap();

        assert_ne!(checksum1, checksum2);
    }

    #[test]
    fn test_is_valid_no_entry() {
        let cache = Cache::new();
        assert!(!cache.is_valid("test_msgs", "abc123"));
    }

    #[test]
    fn test_is_valid_checksum_mismatch() {
        let mut cache = Cache::new();
        let temp_dir = tempfile::tempdir().unwrap();
        cache.insert(CacheEntry {
            package_name: "test_msgs".to_string(),
            checksum: "abc123".to_string(),
            ros_distro: None,
            package_version: None,
            timestamp: 1234567890,
            output_dir: temp_dir.path().to_path_buf(),
        });

        assert!(!cache.is_valid("test_msgs", "different_checksum"));
    }

    #[test]
    fn test_is_valid_output_missing() {
        let mut cache = Cache::new();
        cache.insert(CacheEntry {
            package_name: "test_msgs".to_string(),
            checksum: "abc123".to_string(),
            ros_distro: None,
            package_version: None,
            timestamp: 1234567890,
            output_dir: PathBuf::from("/nonexistent/path"),
        });

        assert!(!cache.is_valid("test_msgs", "abc123"));
    }
}
