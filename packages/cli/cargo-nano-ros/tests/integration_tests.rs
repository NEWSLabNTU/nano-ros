//! Integration tests for cargo-ros2 workflow
//!
//! These tests verify the complete end-to-end workflow including:
//! - Dependency discovery
//! - Cache behavior
//! - Binding generation
//! - Config patching
//!
//! Note: Some tests require ROS 2 to be sourced (AMENT_PREFIX_PATH set)

use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

// Helper to check if ROS is available
fn is_ros_available() -> bool {
    std::env::var("AMENT_PREFIX_PATH").is_ok()
}

// Get path to test fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

// Helper to copy a test project fixture to a temp directory
fn copy_test_project(fixture_name: &str, dest: &Path) -> std::io::Result<()> {
    let fixture_path = fixtures_dir().join(fixture_name);

    // Copy directory recursively
    copy_dir_recursive(&fixture_path, dest)?;

    Ok(())
}

// Recursive directory copy helper
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[test]
fn test_workflow_context_creation() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().to_path_buf();

    // Copy basic project fixture
    copy_test_project("basic_project", temp_dir.path()).unwrap();

    // Verify the project structure
    assert!(project_root.join("Cargo.toml").exists());
    assert!(project_root.join("src").join("main.rs").exists());
}

#[test]
fn test_dependency_discovery_no_ros_deps() {
    let temp_dir = TempDir::new().unwrap();

    // Copy project with non-ROS dependencies
    copy_test_project("project_with_deps", temp_dir.path()).unwrap();

    // This test just verifies we can parse a project without ROS deps
    // The actual workflow would discover zero ROS dependencies
    assert!(temp_dir.path().join("Cargo.toml").exists());
}

#[test]
fn test_cache_file_location() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().to_path_buf();

    // The cache file should be at project_root/.ros2_bindgen_cache
    let expected_cache = project_root.join(".ros2_bindgen_cache");

    // For now, just verify the path logic
    assert_eq!(expected_cache.file_name().unwrap(), ".ros2_bindgen_cache");
}

#[test]
fn test_output_directory_structure() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().to_path_buf();

    // Output directory should be target/ros2_bindings
    let expected_output = project_root.join("target").join("ros2_bindings");

    assert_eq!(expected_output.file_name().unwrap(), "ros2_bindings");
    assert_eq!(
        expected_output.parent().unwrap().file_name().unwrap(),
        "target"
    );
}

#[test]
fn test_cargo_config_path() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().to_path_buf();

    // Config should be at .cargo/config.toml
    let expected_config = project_root.join(".cargo").join("config.toml");

    assert_eq!(expected_config.file_name().unwrap(), "config.toml");
    assert_eq!(
        expected_config.parent().unwrap().file_name().unwrap(),
        ".cargo"
    );
}

// Tests that require ROS to be sourced
mod with_ros {
    use super::*;

    #[test]
    fn test_ament_index_available() {
        if !is_ros_available() {
            eprintln!("Skipping test: ROS not sourced");
            return;
        }

        // If ROS is sourced, AMENT_PREFIX_PATH should be set
        let ament_path = std::env::var("AMENT_PREFIX_PATH").unwrap();
        assert!(!ament_path.is_empty());
        eprintln!("AMENT_PREFIX_PATH: {}", ament_path);
    }

    #[test]
    fn test_discover_ament_packages() {
        if !is_ros_available() {
            eprintln!("Skipping test: ROS not sourced");
            return;
        }

        // This test verifies we can discover ROS packages
        // We'll use the actual ament index
        use rosidl_bindgen::ament::AmentIndex;

        let index = AmentIndex::from_env();
        assert!(index.is_ok(), "Failed to load ament index");

        let index = index.unwrap();
        let package_count = index.package_count();

        eprintln!("Found {} ROS packages", package_count);
        assert!(package_count > 0, "No ROS packages found");
    }

    #[test]
    fn test_find_std_msgs() {
        if !is_ros_available() {
            eprintln!("Skipping test: ROS not sourced");
            return;
        }

        use rosidl_bindgen::ament::AmentIndex;

        let index = AmentIndex::from_env().unwrap();
        let std_msgs = index.find_package("std_msgs");

        if let Some(package) = std_msgs {
            eprintln!("Found std_msgs at: {}", package.share_dir.display());
            assert!(package.share_dir.exists());
            assert_eq!(package.name, "std_msgs");
        } else {
            eprintln!("Warning: std_msgs not found (is it installed?)");
        }
    }
}

// Cache behavior tests
mod cache_tests {
    use super::*;

    #[test]
    fn test_cache_cold_start() {
        let temp_dir = TempDir::new().unwrap();
        let cache_file = temp_dir.path().join(".ros2_bindgen_cache");

        // Cache file shouldn't exist initially
        assert!(!cache_file.exists());

        // After loading (which creates empty cache), it still shouldn't exist
        // until we save it
        use cargo_nano_ros::cache::Cache;
        let cache = Cache::load(&cache_file).unwrap();

        assert_eq!(cache.len(), 0);
        assert!(!cache_file.exists()); // Not saved yet
    }

    #[test]
    fn test_cache_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let cache_file = temp_dir.path().join(".ros2_bindgen_cache");

        use cargo_nano_ros::cache::{Cache, CacheEntry};
        use std::time::{SystemTime, UNIX_EPOCH};

        // Create and save cache
        let mut cache = Cache::load(&cache_file).unwrap();

        let entry = CacheEntry {
            package_name: "test_msgs".to_string(),
            checksum: "abc123".to_string(),
            ros_distro: Some("humble".to_string()),
            package_version: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            output_dir: temp_dir.path().join("output"),
        };

        cache.insert(entry);
        cache.save(&cache_file).unwrap();

        // Verify file exists
        assert!(cache_file.exists());

        // Load cache again
        let cache2 = Cache::load(&cache_file).unwrap();
        assert_eq!(cache2.len(), 1);

        let loaded_entry = cache2.get("test_msgs").unwrap();
        assert_eq!(loaded_entry.package_name, "test_msgs");
        assert_eq!(loaded_entry.checksum, "abc123");
    }

    #[test]
    fn test_cache_invalidation_checksum_change() {
        let temp_dir = TempDir::new().unwrap();
        let cache_file = temp_dir.path().join(".ros2_bindgen_cache");
        let output_dir = temp_dir.path().join("output");

        use cargo_nano_ros::cache::{Cache, CacheEntry};
        use std::time::{SystemTime, UNIX_EPOCH};

        // Create output directory
        fs::create_dir_all(&output_dir).unwrap();

        // Create cache with entry
        let mut cache = Cache::load(&cache_file).unwrap();

        let entry = CacheEntry {
            package_name: "test_msgs".to_string(),
            checksum: "old_checksum".to_string(),
            ros_distro: Some("humble".to_string()),
            package_version: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            output_dir: output_dir.clone(),
        };

        cache.insert(entry);
        cache.save(&cache_file).unwrap();

        // Check validity with different checksum
        let is_valid = cache.is_valid("test_msgs", "new_checksum");
        assert!(!is_valid, "Cache should be invalid with different checksum");

        // Check validity with same checksum
        let is_valid = cache.is_valid("test_msgs", "old_checksum");
        assert!(is_valid, "Cache should be valid with same checksum");
    }

    #[test]
    fn test_cache_invalidation_missing_output() {
        let temp_dir = TempDir::new().unwrap();
        let cache_file = temp_dir.path().join(".ros2_bindgen_cache");
        let output_dir = temp_dir.path().join("output");

        // Note: NOT creating output directory

        use cargo_nano_ros::cache::{Cache, CacheEntry};
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut cache = Cache::load(&cache_file).unwrap();

        let entry = CacheEntry {
            package_name: "test_msgs".to_string(),
            checksum: "abc123".to_string(),
            ros_distro: Some("humble".to_string()),
            package_version: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            output_dir,
        };

        cache.insert(entry);

        // Check validity - should be invalid because output dir doesn't exist
        let is_valid = cache.is_valid("test_msgs", "abc123");
        assert!(
            !is_valid,
            "Cache should be invalid when output directory missing"
        );
    }
}

// Config patcher tests
mod config_patcher_tests {
    use super::*;

    #[test]
    fn test_create_new_config() {
        let temp_dir = TempDir::new().unwrap();

        use cargo_nano_ros::config_patcher::ConfigPatcher;

        let patcher = ConfigPatcher::new(temp_dir.path()).unwrap();

        // Add a patch
        let patch_path = temp_dir.path().join("bindings").join("std_msgs");
        let mut patcher = patcher;
        patcher.add_patch("std_msgs", &patch_path);

        // Save
        patcher.save().unwrap();

        // Verify config file exists
        let config_path = temp_dir.path().join(".cargo").join("config.toml");
        assert!(config_path.exists());

        // Verify content
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("std_msgs"));
        assert!(content.contains("patch"));
    }

    #[test]
    fn test_preserve_existing_config() {
        let temp_dir = TempDir::new().unwrap();
        let cargo_dir = temp_dir.path().join(".cargo");
        fs::create_dir_all(&cargo_dir).unwrap();

        // Create existing config
        let config_path = cargo_dir.join("config.toml");
        fs::write(
            &config_path,
            "[build]\ntarget = \"x86_64-unknown-linux-gnu\"\n",
        )
        .unwrap();

        use cargo_nano_ros::config_patcher::ConfigPatcher;

        let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();

        let patch_path = temp_dir.path().join("bindings").join("std_msgs");
        patcher.add_patch("std_msgs", &patch_path);
        patcher.save().unwrap();

        // Verify existing content is preserved
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("target = \"x86_64-unknown-linux-gnu\""));
        assert!(content.contains("std_msgs"));
    }

    #[test]
    fn test_update_existing_patch() {
        let temp_dir = TempDir::new().unwrap();

        use cargo_nano_ros::config_patcher::ConfigPatcher;

        let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();

        // Add initial patch
        let path1 = temp_dir.path().join("bindings1").join("std_msgs");
        patcher.add_patch("std_msgs", &path1);
        patcher.save().unwrap();

        // Update patch
        let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();
        let path2 = temp_dir.path().join("bindings2").join("std_msgs");
        patcher.add_patch("std_msgs", &path2);
        patcher.save().unwrap();

        // Verify only latest patch exists
        let config_path = temp_dir.path().join(".cargo").join("config.toml");
        let content = fs::read_to_string(&config_path).unwrap();

        assert!(content.contains("bindings2"));
        assert!(!content.contains("bindings1"));
    }

    #[test]
    fn test_remove_patch() {
        let temp_dir = TempDir::new().unwrap();

        use cargo_nano_ros::config_patcher::ConfigPatcher;

        let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();

        // Add patches
        let path1 = temp_dir.path().join("bindings").join("std_msgs");
        let path2 = temp_dir.path().join("bindings").join("geometry_msgs");
        patcher.add_patch("std_msgs", &path1);
        patcher.add_patch("geometry_msgs", &path2);
        patcher.save().unwrap();

        // Remove one patch
        let mut patcher = ConfigPatcher::new(temp_dir.path()).unwrap();
        patcher.remove_patch("std_msgs");
        patcher.save().unwrap();

        // Verify
        let config_path = temp_dir.path().join(".cargo").join("config.toml");
        let content = fs::read_to_string(&config_path).unwrap();

        assert!(!content.contains("std_msgs"));
        assert!(content.contains("geometry_msgs"));
    }
}

// Dependency parser tests with real cargo metadata
mod dependency_parser_tests {
    use super::*;

    #[test]
    fn test_discover_no_ros_dependencies() {
        let temp_dir = TempDir::new().unwrap();

        // Copy project with non-ROS deps
        copy_test_project("project_with_deps", temp_dir.path()).unwrap();

        use cargo_nano_ros::dependency_parser::DependencyParser;
        use std::collections::HashSet;

        let mut known_ros = HashSet::new();
        known_ros.insert("std_msgs".to_string());

        let parser = DependencyParser::new(known_ros);
        let deps = parser.discover_dependencies(temp_dir.path()).unwrap();

        assert_eq!(deps.len(), 0, "Should find no ROS dependencies");
    }

    #[test]
    fn test_parse_valid_cargo_toml() {
        let temp_dir = TempDir::new().unwrap();

        // Copy basic project fixture
        copy_test_project("basic_project", temp_dir.path()).unwrap();

        use cargo_nano_ros::dependency_parser::DependencyParser;
        use std::collections::HashSet;

        let parser = DependencyParser::new(HashSet::new());
        let result = parser.discover_dependencies(temp_dir.path());

        assert!(result.is_ok(), "Should parse valid Cargo.toml");
    }
}

// Error handling tests
mod error_tests {
    use super::*;

    #[test]
    fn test_missing_cargo_toml() {
        let temp_dir = TempDir::new().unwrap();

        // Don't create Cargo.toml

        use cargo_nano_ros::dependency_parser::DependencyParser;
        use std::collections::HashSet;

        let parser = DependencyParser::new(HashSet::new());
        let result = parser.discover_dependencies(temp_dir.path());

        assert!(result.is_err(), "Should fail with missing Cargo.toml");
    }

    #[test]
    fn test_ament_index_not_available() {
        // Temporarily unset AMENT_PREFIX_PATH if it exists
        let original = std::env::var("AMENT_PREFIX_PATH").ok();
        unsafe { std::env::remove_var("AMENT_PREFIX_PATH") };

        use rosidl_bindgen::ament::AmentIndex;

        let result = AmentIndex::from_env();

        // Restore original value
        if let Some(val) = original {
            unsafe { std::env::set_var("AMENT_PREFIX_PATH", val) };
        }

        assert!(
            result.is_err(),
            "Should fail when AMENT_PREFIX_PATH not set"
        );
        if let Err(e) = result {
            let err_str = e.to_string();
            assert!(
                err_str.contains("AMENT_PREFIX_PATH") || err_str.contains("environment variable"),
                "Error should mention AMENT_PREFIX_PATH, got: {}",
                err_str
            );
        }
    }
}

// CLI command tests
mod cli_tests {
    use super::*;

    #[test]
    fn test_cache_list_empty() {
        let temp_dir = TempDir::new().unwrap();

        use cargo_nano_ros::cache::Cache;

        // Empty cache
        let cache = Cache::load(&temp_dir.path().join(".ros2_bindgen_cache")).unwrap();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_list_with_entries() {
        let temp_dir = TempDir::new().unwrap();
        let cache_file = temp_dir.path().join(".ros2_bindgen_cache");

        use cargo_nano_ros::cache::{Cache, CacheEntry};
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut cache = Cache::load(&cache_file).unwrap();

        // Add entries
        let entry1 = CacheEntry {
            package_name: "std_msgs".to_string(),
            checksum: "abc123".to_string(),
            ros_distro: Some("humble".to_string()),
            package_version: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            output_dir: temp_dir.path().join("std_msgs"),
        };

        let entry2 = CacheEntry {
            package_name: "geometry_msgs".to_string(),
            checksum: "def456".to_string(),
            ros_distro: Some("humble".to_string()),
            package_version: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            output_dir: temp_dir.path().join("geometry_msgs"),
        };

        cache.insert(entry1);
        cache.insert(entry2);
        cache.save(&cache_file).unwrap();

        // Verify entries can be iterated
        let cache = Cache::load(&cache_file).unwrap();
        assert_eq!(cache.len(), 2);

        let entries: Vec<_> = cache.entries().collect();
        assert_eq!(entries.len(), 2);

        // Verify both packages are present
        assert!(cache.get("std_msgs").is_some());
        assert!(cache.get("geometry_msgs").is_some());
    }

    #[test]
    fn test_cache_rebuild_removes_entry() {
        let temp_dir = TempDir::new().unwrap();
        let cache_file = temp_dir.path().join(".ros2_bindgen_cache");

        use cargo_nano_ros::cache::{Cache, CacheEntry};
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut cache = Cache::load(&cache_file).unwrap();

        let entry = CacheEntry {
            package_name: "std_msgs".to_string(),
            checksum: "abc123".to_string(),
            ros_distro: Some("humble".to_string()),
            package_version: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            output_dir: temp_dir.path().join("std_msgs"),
        };

        cache.insert(entry);
        cache.save(&cache_file).unwrap();

        // Verify entry exists
        let cache = Cache::load(&cache_file).unwrap();
        assert!(cache.get("std_msgs").is_some());

        // Remove entry (simulating rebuild command)
        let mut cache = Cache::load(&cache_file).unwrap();
        cache.remove("std_msgs");
        cache.save(&cache_file).unwrap();

        // Verify entry is gone
        let cache = Cache::load(&cache_file).unwrap();
        assert!(cache.get("std_msgs").is_none());
    }

    #[test]
    fn test_cache_entries_iterator() {
        let temp_dir = TempDir::new().unwrap();
        let cache_file = temp_dir.path().join(".ros2_bindgen_cache");

        use cargo_nano_ros::cache::{Cache, CacheEntry};
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut cache = Cache::load(&cache_file).unwrap();

        // Add multiple entries
        for i in 0..5 {
            let entry = CacheEntry {
                package_name: format!("package_{}", i),
                checksum: format!("checksum_{}", i),
                ros_distro: Some("humble".to_string()),
                package_version: None,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                output_dir: temp_dir.path().join(format!("package_{}", i)),
            };
            cache.insert(entry);
        }

        cache.save(&cache_file).unwrap();

        // Load and iterate
        let cache = Cache::load(&cache_file).unwrap();
        let entries: Vec<_> = cache.entries().collect();

        assert_eq!(entries.len(), 5);

        // Verify all packages are present
        for i in 0..5 {
            let package_name = format!("package_{}", i);
            assert!(
                entries.iter().any(|e| e.package_name == package_name),
                "Package {} not found",
                package_name
            );
        }
    }
}

// Phase 172 W.3 — planned-mode component scaffold.
mod component_scaffold {
    use super::*;
    use cargo_nano_ros::scaffold::{ComponentScaffoldConfig, scaffold_component};
    use std::sync::Mutex;

    // The scaffolder writes cwd-relative (matching `scaffold_package`), so the
    // test chdir's; serialize so it never races a sibling that does the same.
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn scaffold_component_emits_folded_minimal_manifest_and_lib() {
        let _g = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = TempDir::new().unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = scaffold_component(&ComponentScaffoldConfig {
            name: "my-comp".to_string(),
            use_case: "talker".to_string(),
            lang: "rust".to_string(),
            force: false,
        });
        std::env::set_current_dir(&prev).unwrap();
        result.expect("scaffold_component");

        let dir = tmp.path().join("my-comp");

        // Folded `[component]` manifest with a 2-segment `crate::module` id
        // (`nros metadata --build` requires it), and intentionally minimal —
        // no `[linkage]` (derived) / `[overrides]` (defaulted), per W.3.
        let nros_toml = fs::read_to_string(dir.join("nros.toml")).unwrap();
        assert!(nros_toml.contains("[component]"));
        assert!(nros_toml.contains(r#"component = "my_comp::talker""#));
        assert!(nros_toml.contains(r#"package = "my-comp""#));
        assert!(nros_toml.contains(r#"source_metadata = "metadata/talker.json""#));
        assert!(
            !nros_toml.contains("[linkage]") && !nros_toml.contains("[overrides]"),
            "manifest must stay minimal: {nros_toml}"
        );

        // A real `nros::Component` impl in a `crate::talker` module.
        let lib = fs::read_to_string(dir.join("src/lib.rs")).unwrap();
        assert!(lib.contains("impl nros::Component for Component"));
        assert!(lib.contains(r#"const NAME: &'static str = "talker""#));
        assert!(lib.contains("pub mod talker"));

        // A library crate (rlib), not a binary.
        let cargo = fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("nros = {"));
        assert!(
            !cargo.contains("[[bin]]"),
            "component is a library: {cargo}"
        );
        assert!(dir.join("package.xml").is_file());
    }

    #[test]
    fn scaffold_component_uses_the_use_case_as_the_node_module() {
        let _g = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = TempDir::new().unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = scaffold_component(&ComponentScaffoldConfig {
            name: "svc".to_string(),
            use_case: "service".to_string(),
            lang: "rust".to_string(),
            force: false,
        });
        std::env::set_current_dir(&prev).unwrap();
        result.expect("scaffold_component");

        let nros_toml = fs::read_to_string(tmp.path().join("svc/nros.toml")).unwrap();
        assert!(nros_toml.contains(r#"component = "svc::service""#));
        let lib = fs::read_to_string(tmp.path().join("svc/src/lib.rs")).unwrap();
        assert!(lib.contains("pub mod service"));
    }

    #[test]
    fn scaffold_component_c_emits_node_pkg_shape() {
        let _g = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = TempDir::new().unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = scaffold_component(&ComponentScaffoldConfig {
            name: "c-talker".to_string(),
            use_case: "talker".to_string(),
            lang: "c".to_string(),
            force: false,
        });
        std::env::set_current_dir(&prev).unwrap();
        result.expect("scaffold_component c");

        let dir = tmp.path().join("c-talker");
        let cmake = fs::read_to_string(dir.join("CMakeLists.txt")).unwrap();
        assert!(cmake.contains("project(c_talker VERSION 0.1.0 LANGUAGES C CXX)"));
        // Typed C component (RFC-0043): raw publisher, no generated bindings.
        assert!(cmake.contains("LANGUAGE C"));
        assert!(cmake.contains("TYPED"));
        assert!(cmake.contains("CLASS    c_talker::Talker"));

        let source = fs::read_to_string(dir.join("src/Talker.c")).unwrap();
        assert!(source.contains("NROS_C_COMPONENT(c_talker_t, talker_configure)"));
        assert!(source.contains("nros_cpp_publisher_create("));
        assert!(source.contains("nros_cpp_timer_create("));
        assert!(
            !source.contains("NROS_NODE_REGISTER"),
            "typed C scaffold must not emit the legacy declarative seam:\n{source}"
        );
        assert!(dir.join("package.xml").is_file());
    }

    #[test]
    fn scaffold_component_cpp_emits_typed_component() {
        let _g = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = TempDir::new().unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = scaffold_component(&ComponentScaffoldConfig {
            name: "cpp-talker".to_string(),
            use_case: "talker".to_string(),
            lang: "cpp".to_string(),
            force: false,
        });
        std::env::set_current_dir(&prev).unwrap();
        result.expect("scaffold_component cpp");

        let dir = tmp.path().join("cpp-talker");
        let cmake = fs::read_to_string(dir.join("CMakeLists.txt")).unwrap();
        assert!(cmake.contains("CLASS   cpp_talker::Talker"));
        assert!(cmake.contains("std_msgs__nano_ros_cpp"));

        // Header at include/<pkg>/<Class>.hpp (typed Entry includes it by that path).
        let hpp = fs::read_to_string(dir.join("include/cpp_talker/Talker.hpp")).unwrap();
        assert!(hpp.contains("::nros::Result configure(::nros::Node& node);"));
        assert!(hpp.contains("#include <nros/component.hpp>"));
        assert!(
            !hpp.contains("register_node") && !hpp.contains("NodeContext"),
            "typed C++ scaffold header must not carry the declarative seam:\n{hpp}"
        );

        let cpp = fs::read_to_string(dir.join("src/Talker.cpp")).unwrap();
        assert!(cpp.contains("::nros::Result Talker::configure(::nros::Node& node)"));
        assert!(cpp.contains("bind_timer<Talker, &Talker::on_tick>"));
        assert!(
            !cpp.contains("NROS_NODE_REGISTER") && !cpp.contains("DeclaredNode"),
            "typed C++ scaffold body must not carry the declarative seam:\n{cpp}"
        );
        assert!(dir.join("package.xml").is_file());
    }
}
