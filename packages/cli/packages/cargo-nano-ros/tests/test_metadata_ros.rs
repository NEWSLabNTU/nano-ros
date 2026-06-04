use std::fs;
use tempfile::TempDir;

#[test]
fn test_metadata_ros_install_to_share() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().join("project");
    let install_base = temp_dir.path().join("install");
    let target_dir = temp_dir.path().join("target");

    // Create project structure
    fs::create_dir_all(&project_root).unwrap();
    fs::create_dir_all(project_root.join("src")).unwrap();
    fs::create_dir_all(project_root.join("launch")).unwrap();
    fs::create_dir_all(project_root.join("config")).unwrap();

    // Create files
    fs::write(project_root.join("src/lib.rs"), "").unwrap();
    fs::write(project_root.join("launch/test.launch.xml"), "<launch/>").unwrap();
    fs::write(project_root.join("config/params.yaml"), "{}").unwrap();

    // Create Cargo.toml with metadata.ros
    fs::write(
        project_root.join("Cargo.toml"),
        r#"
[package]
name = "test-pkg"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_pkg"

[package.metadata.ros]
install_to_share = ["launch", "config"]
"#,
    )
    .unwrap();

    // Create package.xml
    fs::write(
        project_root.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="2">
  <name>test_pkg</name>
  <version>0.1.0</version>
  <description>Test package</description>
  <maintainer email="test@test.com">Test</maintainer>
  <license>MIT</license>
</package>
"#,
    )
    .unwrap();

    // Create installer
    let installer = cargo_nano_ros::ament_installer::AmentInstaller::new(
        install_base.clone(),
        "test-pkg".to_string(),
        project_root.clone(),
        target_dir,
        false,
        "debug".to_string(),
    );

    // Run installation
    installer.install(true).unwrap();

    // Verify files were installed
    let share_dir = install_base.join("share").join("test-pkg");
    assert!(share_dir.join("launch").join("test.launch.xml").exists());
    assert!(share_dir.join("config").join("params.yaml").exists());
}

#[test]
fn test_metadata_ros_install_to_include() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().join("project");
    let install_base = temp_dir.path().join("install");
    let target_dir = temp_dir.path().join("target");

    // Create project structure
    fs::create_dir_all(&project_root).unwrap();
    fs::create_dir_all(project_root.join("src")).unwrap();
    fs::create_dir_all(project_root.join("include")).unwrap();

    // Create files
    fs::write(project_root.join("src/lib.rs"), "").unwrap();
    fs::write(project_root.join("include/test.hpp"), "#pragma once").unwrap();

    // Create Cargo.toml with metadata.ros
    fs::write(
        project_root.join("Cargo.toml"),
        r#"
[package]
name = "test-pkg"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_pkg"

[package.metadata.ros]
install_to_include = ["include"]
"#,
    )
    .unwrap();

    // Create package.xml
    fs::write(
        project_root.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="2">
  <name>test_pkg</name>
  <version>0.1.0</version>
  <description>Test package</description>
  <maintainer email="test@test.com">Test</maintainer>
  <license>MIT</license>
</package>
"#,
    )
    .unwrap();

    // Create installer
    let installer = cargo_nano_ros::ament_installer::AmentInstaller::new(
        install_base.clone(),
        "test-pkg".to_string(),
        project_root.clone(),
        target_dir,
        false,
        "debug".to_string(),
    );

    // Run installation
    installer.install(true).unwrap();

    // Verify files were installed
    let include_dir = install_base.join("include").join("test-pkg");
    assert!(include_dir.join("include").join("test.hpp").exists());
}

#[test]
fn test_metadata_ros_install_to_lib() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().join("project");
    let install_base = temp_dir.path().join("install");
    let target_dir = temp_dir.path().join("target");

    // Create project structure
    fs::create_dir_all(&project_root).unwrap();
    fs::create_dir_all(project_root.join("src")).unwrap();
    fs::create_dir_all(project_root.join("scripts")).unwrap();

    // Create files
    fs::write(project_root.join("src/lib.rs"), "").unwrap();
    fs::write(project_root.join("scripts/setup.sh"), "#!/bin/bash").unwrap();

    // Create Cargo.toml with metadata.ros
    fs::write(
        project_root.join("Cargo.toml"),
        r#"
[package]
name = "test-pkg"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_pkg"

[package.metadata.ros]
install_to_lib = ["scripts"]
"#,
    )
    .unwrap();

    // Create package.xml
    fs::write(
        project_root.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="2">
  <name>test_pkg</name>
  <version>0.1.0</version>
  <description>Test package</description>
  <maintainer email="test@test.com">Test</maintainer>
  <license>MIT</license>
</package>
"#,
    )
    .unwrap();

    // Create installer
    let installer = cargo_nano_ros::ament_installer::AmentInstaller::new(
        install_base.clone(),
        "test-pkg".to_string(),
        project_root.clone(),
        target_dir,
        false,
        "debug".to_string(),
    );

    // Run installation
    installer.install(true).unwrap();

    // Verify files were installed
    let lib_dir = install_base.join("lib").join("test-pkg");
    assert!(lib_dir.join("scripts").join("setup.sh").exists());
}

#[test]
fn test_metadata_ros_install_individual_file_to_share() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().join("project");
    let install_base = temp_dir.path().join("install");
    let target_dir = temp_dir.path().join("target");

    // Create project structure
    fs::create_dir_all(&project_root).unwrap();
    fs::create_dir_all(project_root.join("src")).unwrap();

    // Create individual file (not in a directory)
    fs::write(project_root.join("src/lib.rs"), "").unwrap();
    fs::write(project_root.join("README.md"), "# Test Package").unwrap();

    // Create Cargo.toml with individual file in metadata.ros
    fs::write(
        project_root.join("Cargo.toml"),
        r#"
[package]
name = "test-pkg"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_pkg"

[package.metadata.ros]
install_to_share = ["README.md"]
"#,
    )
    .unwrap();

    // Create package.xml
    fs::write(
        project_root.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="2">
  <name>test_pkg</name>
  <version>0.1.0</version>
  <description>Test package</description>
  <maintainer email="test@test.com">Test</maintainer>
  <license>MIT</license>
</package>
"#,
    )
    .unwrap();

    // Create installer
    let installer = cargo_nano_ros::ament_installer::AmentInstaller::new(
        install_base.clone(),
        "test-pkg".to_string(),
        project_root.clone(),
        target_dir,
        false,
        "debug".to_string(),
    );

    // Run installation
    installer.install(true).unwrap();

    // Verify file was installed (only filename, no parent directory)
    let share_dir = install_base.join("share").join("test-pkg");
    assert!(share_dir.join("README.md").exists());
}

#[test]
fn test_metadata_ros_install_individual_file_to_include() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().join("project");
    let install_base = temp_dir.path().join("install");
    let target_dir = temp_dir.path().join("target");

    // Create project structure
    fs::create_dir_all(&project_root).unwrap();
    fs::create_dir_all(project_root.join("src")).unwrap();

    // Create individual header file
    fs::write(project_root.join("src/lib.rs"), "").unwrap();
    fs::write(project_root.join("mylib.h"), "#pragma once").unwrap();

    // Create Cargo.toml with individual file in metadata.ros
    fs::write(
        project_root.join("Cargo.toml"),
        r#"
[package]
name = "test-pkg"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_pkg"

[package.metadata.ros]
install_to_include = ["mylib.h"]
"#,
    )
    .unwrap();

    // Create package.xml
    fs::write(
        project_root.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="2">
  <name>test_pkg</name>
  <version>0.1.0</version>
  <description>Test package</description>
  <maintainer email="test@test.com">Test</maintainer>
  <license>MIT</license>
</package>
"#,
    )
    .unwrap();

    // Create installer
    let installer = cargo_nano_ros::ament_installer::AmentInstaller::new(
        install_base.clone(),
        "test-pkg".to_string(),
        project_root.clone(),
        target_dir,
        false,
        "debug".to_string(),
    );

    // Run installation
    installer.install(true).unwrap();

    // Verify file was installed
    let include_dir = install_base.join("include").join("test-pkg");
    assert!(include_dir.join("mylib.h").exists());
}

#[test]
fn test_metadata_ros_install_individual_file_to_lib() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().join("project");
    let install_base = temp_dir.path().join("install");
    let target_dir = temp_dir.path().join("target");

    // Create project structure
    fs::create_dir_all(&project_root).unwrap();
    fs::create_dir_all(project_root.join("src")).unwrap();

    // Create individual script file
    fs::write(project_root.join("src/lib.rs"), "").unwrap();
    fs::write(project_root.join("setup.sh"), "#!/bin/bash").unwrap();

    // Create Cargo.toml with individual file in metadata.ros
    fs::write(
        project_root.join("Cargo.toml"),
        r#"
[package]
name = "test-pkg"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_pkg"

[package.metadata.ros]
install_to_lib = ["setup.sh"]
"#,
    )
    .unwrap();

    // Create package.xml
    fs::write(
        project_root.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="2">
  <name>test_pkg</name>
  <version>0.1.0</version>
  <description>Test package</description>
  <maintainer email="test@test.com">Test</maintainer>
  <license>MIT</license>
</package>
"#,
    )
    .unwrap();

    // Create installer
    let installer = cargo_nano_ros::ament_installer::AmentInstaller::new(
        install_base.clone(),
        "test-pkg".to_string(),
        project_root.clone(),
        target_dir,
        false,
        "debug".to_string(),
    );

    // Run installation
    installer.install(true).unwrap();

    // Verify file was installed
    let lib_dir = install_base.join("lib").join("test-pkg");
    assert!(lib_dir.join("setup.sh").exists());
}

#[test]
fn test_metadata_ros_mixed_files_and_dirs() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().join("project");
    let install_base = temp_dir.path().join("install");
    let target_dir = temp_dir.path().join("target");

    // Create project structure with both files and directories
    fs::create_dir_all(&project_root).unwrap();
    fs::create_dir_all(project_root.join("src")).unwrap();
    fs::create_dir_all(project_root.join("launch")).unwrap();
    fs::create_dir_all(project_root.join("config")).unwrap();

    // Create files
    fs::write(project_root.join("src/lib.rs"), "").unwrap();
    fs::write(project_root.join("launch/test.launch.xml"), "<launch/>").unwrap();
    fs::write(project_root.join("config/params.yaml"), "{}").unwrap();
    fs::write(project_root.join("README.md"), "# Test").unwrap();
    fs::write(project_root.join("LICENSE"), "MIT").unwrap();

    // Create Cargo.toml with mixed files and directories
    fs::write(
        project_root.join("Cargo.toml"),
        r#"
[package]
name = "test-pkg"
version = "0.1.0"
edition = "2021"

[lib]
name = "test_pkg"

[package.metadata.ros]
install_to_share = ["launch", "config", "README.md", "LICENSE"]
"#,
    )
    .unwrap();

    // Create package.xml
    fs::write(
        project_root.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="2">
  <name>test_pkg</name>
  <version>0.1.0</version>
  <description>Test package</description>
  <maintainer email="test@test.com">Test</maintainer>
  <license>MIT</license>
</package>
"#,
    )
    .unwrap();

    // Create installer
    let installer = cargo_nano_ros::ament_installer::AmentInstaller::new(
        install_base.clone(),
        "test-pkg".to_string(),
        project_root.clone(),
        target_dir,
        false,
        "debug".to_string(),
    );

    // Run installation
    installer.install(true).unwrap();

    // Verify directories were installed
    let share_dir = install_base.join("share").join("test-pkg");
    assert!(share_dir.join("launch").join("test.launch.xml").exists());
    assert!(share_dir.join("config").join("params.yaml").exists());

    // Verify individual files were installed
    assert!(share_dir.join("README.md").exists());
    assert!(share_dir.join("LICENSE").exists());
}
