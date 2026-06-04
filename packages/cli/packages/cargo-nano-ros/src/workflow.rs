//! Main workflow orchestration for cargo-ros2
//!
//! This module coordinates the entire process:
//! 1. Discover ROS dependencies from Cargo.toml
//! 2. Check cache for each package
//! 3. Generate missing/stale bindings
//! 4. Update cache
//! 5. Patch .cargo/config.toml
//! 6. Invoke cargo build

use crate::{
    cache::{self, CACHE_FILE_NAME, Cache, CacheEntry},
    config_patcher::ConfigPatcher,
    dependency_parser::{DependencyParser, RosDependency},
};
use eyre::{Result, WrapErr, eyre};
use rosidl_bindgen::ament::AmentIndex;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

/// Workflow context
pub struct WorkflowContext {
    /// Project root directory
    pub project_root: PathBuf,
    /// Output directory for bindings (default: target/ros2_bindings)
    pub output_dir: PathBuf,
    /// Cache file path
    pub cache_file: PathBuf,
    /// Verbose output
    pub verbose: bool,
}

impl WorkflowContext {
    /// Create a new workflow context
    pub fn new(project_root: PathBuf, verbose: bool) -> Self {
        let output_dir = project_root.join("target").join("ros2_bindings");
        let cache_file = project_root.join(CACHE_FILE_NAME);

        WorkflowContext {
            project_root,
            output_dir,
            cache_file,
            verbose,
        }
    }

    /// Create a workspace-level workflow context
    ///
    /// This generates bindings in the workspace root's build/ directory,
    /// allowing all packages in the workspace to share the same bindings.
    /// This follows ROS 2/colcon conventions where build artifacts go in build/.
    ///
    /// # Arguments
    /// * `workspace_root` - The colcon workspace root directory
    /// * `project_root` - The current package's root directory
    /// * `verbose` - Enable verbose output
    pub fn new_workspace_level(
        workspace_root: PathBuf,
        project_root: PathBuf,
        verbose: bool,
    ) -> Self {
        let output_dir = workspace_root.join("build").join("ros2_bindings");
        let cache_file = workspace_root.join("build").join(CACHE_FILE_NAME);

        WorkflowContext {
            project_root,
            output_dir,
            cache_file,
            verbose,
        }
    }

    /// Discover ROS dependencies via ament index
    pub fn discover_ament_packages(&self) -> Result<HashMap<String, PathBuf>> {
        let index =
            AmentIndex::from_env().wrap_err("Failed to load ament index (is ROS 2 sourced?)")?;

        let mut packages = HashMap::new();
        for (name, package) in index.packages() {
            packages.insert(name.clone(), package.share_dir.clone());
        }

        Ok(packages)
    }

    /// Discover ROS dependencies from Cargo.toml
    pub fn discover_ros_dependencies(&self) -> Result<Vec<RosDependency>> {
        // Get known ROS packages from ament index
        let ament_packages = self.discover_ament_packages()?;
        let known_ros_packages = ament_packages.keys().cloned().collect();

        // Parse Cargo.toml dependencies
        let parser = DependencyParser::new(known_ros_packages);
        parser.discover_dependencies(&self.project_root)
    }

    /// Check which packages need generation (cache miss or stale)
    pub fn check_cache(
        &self,
        dependencies: &[RosDependency],
        ament_packages: &HashMap<String, PathBuf>,
    ) -> Result<Vec<String>> {
        let cache = Cache::load(&self.cache_file)?;
        let mut to_generate = Vec::new();

        for dep in dependencies {
            // Get the package share dir
            let share_dir = match ament_packages.get(&dep.name) {
                Some(dir) => dir,
                None => {
                    // Package not in ament index, skip
                    continue;
                }
            };

            // Calculate current checksum
            let current_checksum = cache::calculate_package_checksum(share_dir)
                .wrap_err_with(|| format!("Failed to calculate checksum for {}", dep.name))?;

            // Check if cache is valid
            if !cache.is_valid(&dep.name, &current_checksum) {
                to_generate.push(dep.name.clone());
            }
        }

        Ok(to_generate)
    }

    /// Generate bindings for a package using cargo-ros2-bindgen
    pub fn generate_bindings(&self, package_name: &str) -> Result<PathBuf> {
        if self.verbose {
            eprintln!("  Generating bindings for {}...", package_name);
        }

        // Find rosidl-bindgen binary
        let bindgen_binary = self.find_rosidl_bindgen()?;

        // Build command
        let output_path = self.output_dir.clone();
        let mut cmd = Command::new(&bindgen_binary);
        cmd.arg("--package")
            .arg(package_name)
            .arg("--output")
            .arg(&output_path);

        if self.verbose {
            cmd.arg("--verbose");
        }

        // Execute
        let output = cmd
            .output()
            .wrap_err_with(|| format!("Failed to execute {}", bindgen_binary.display()))?;

        if !output.status.success() {
            return Err(eyre!(
                "rosidl-bindgen failed for {}: {}",
                package_name,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(output_path.join(package_name))
    }

    /// Find rosidl-bindgen binary
    fn find_rosidl_bindgen(&self) -> Result<PathBuf> {
        // Try to find in target directory (development)
        let dev_path = self
            .project_root
            .ancestors()
            .find(|p| p.join("Cargo.toml").exists())
            .map(|p| p.join("target").join("debug").join("rosidl-bindgen"));

        if let Some(path) = dev_path
            && path.exists()
        {
            return Ok(path);
        }

        // Try to find in PATH
        if let Ok(output) = Command::new("which").arg("rosidl-bindgen").output()
            && output.status.success()
        {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Ok(PathBuf::from(path_str));
        }

        Err(eyre!(
            "rosidl-bindgen not found. Please build it first with 'cargo build'"
        ))
    }

    /// Update cache after successful generation
    pub fn update_cache(
        &self,
        package_name: &str,
        package_share_dir: &Path,
        output_dir: PathBuf,
    ) -> Result<()> {
        let mut cache = Cache::load(&self.cache_file)?;

        // Calculate checksum of the source package
        let checksum = cache::calculate_package_checksum(package_share_dir)
            .wrap_err_with(|| format!("Failed to calculate checksum for {}", package_name))?;

        let entry = CacheEntry {
            package_name: package_name.to_string(),
            checksum,
            ros_distro: std::env::var("ROS_DISTRO").ok(),
            package_version: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            output_dir,
        };

        cache.insert(entry);
        cache.save(&self.cache_file)?;

        Ok(())
    }

    /// Patch .cargo/config.toml with binding paths
    pub fn patch_cargo_config(&self, packages: &[(String, PathBuf)]) -> Result<()> {
        let mut patcher = ConfigPatcher::new(&self.project_root)?;

        for (package_name, package_path) in packages {
            if self.verbose {
                eprintln!(
                    "  Adding patch for {} -> {}",
                    package_name,
                    package_path.display()
                );
            }
            patcher.add_patch(package_name, package_path);
        }

        patcher.save()?;
        Ok(())
    }

    /// Generate bindings for multiple packages in parallel
    fn generate_bindings_parallel(
        &self,
        packages: &[String],
        ament_packages: &HashMap<String, PathBuf>,
    ) -> Result<Vec<(String, PathBuf)>> {
        use indicatif::{ProgressBar, ProgressStyle};
        use rayon::prelude::*;
        use std::sync::Mutex;

        // Create progress bar
        let pb = ProgressBar::new(packages.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("#>-"),
        );

        // Use Mutex for thread-safe cache updates
        let cache_file = self.cache_file.clone();
        let results = Mutex::new(Vec::new());
        let errors = Mutex::new(Vec::new());

        packages.par_iter().for_each(|package_name| {
            pb.set_message(format!("Generating {}", package_name));

            match self.generate_bindings(package_name) {
                Ok(output_dir) => {
                    // Update cache
                    if let Some(share_dir) = ament_packages.get(package_name)
                        && let Err(e) = self.update_cache_threadsafe(
                            package_name,
                            share_dir,
                            output_dir.clone(),
                            &cache_file,
                        )
                    {
                        errors.lock().unwrap().push(format!(
                            "Failed to update cache for {}: {}",
                            package_name, e
                        ));
                    }

                    results
                        .lock()
                        .unwrap()
                        .push((package_name.clone(), output_dir));
                }
                Err(e) => {
                    errors
                        .lock()
                        .unwrap()
                        .push(format!("Failed to generate {}: {}", package_name, e));
                }
            }

            pb.inc(1);
        });

        pb.finish_with_message("Generation complete");

        // Check for errors
        let errors = errors.lock().unwrap();
        if !errors.is_empty() {
            return Err(eyre!(
                "Errors during parallel generation:\n{}",
                errors.join("\n")
            ));
        }
        drop(errors);

        let results = results.lock().unwrap().clone();
        Ok(results)
    }

    /// Thread-safe cache update for parallel generation
    fn update_cache_threadsafe(
        &self,
        package_name: &str,
        package_share_dir: &Path,
        output_dir: PathBuf,
        cache_file: &Path,
    ) -> Result<()> {
        use std::sync::Mutex;
        static CACHE_LOCK: Mutex<()> = Mutex::new(());

        let _lock = CACHE_LOCK.lock().unwrap();

        let mut cache = Cache::load(cache_file)?;

        // Calculate checksum of the source package
        let checksum = cache::calculate_package_checksum(package_share_dir)
            .wrap_err_with(|| format!("Failed to calculate checksum for {}", package_name))?;

        let entry = CacheEntry {
            package_name: package_name.to_string(),
            checksum,
            ros_distro: std::env::var("ROS_DISTRO").ok(),
            package_version: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            output_dir,
        };

        cache.insert(entry);
        cache.save(cache_file)?;

        Ok(())
    }

    /// Run the complete workflow
    pub fn run(&self, bindings_only: bool) -> Result<()> {
        if self.verbose {
            eprintln!("cargo-ros2 workflow starting...");
        }

        // Step 1: Discover ament packages
        if self.verbose {
            eprintln!("Step 1: Discovering ROS packages from ament index...");
        }
        let ament_packages = self.discover_ament_packages()?;

        if self.verbose {
            eprintln!("  Found {} packages in ament index", ament_packages.len());
        }

        // Step 2: Discover ROS dependencies from Cargo.toml
        if self.verbose {
            eprintln!("Step 2: Discovering ROS dependencies from Cargo.toml...");
        }
        let mut dependencies = self.discover_ros_dependencies()?;

        if dependencies.is_empty() {
            eprintln!("No ROS 2 dependencies found in Cargo.toml");
            if !bindings_only {
                return self.invoke_cargo_build();
            }
            return Ok(());
        }

        if self.verbose {
            eprintln!("  Found {} ROS dependencies", dependencies.len());
        }

        // Step 3: Iteratively discover and generate transitive dependencies
        let mut all_generated = Vec::new();
        let mut seen_packages = std::collections::HashSet::new();

        loop {
            // Check cache for current set of dependencies
            if self.verbose {
                eprintln!(
                    "Step 3: Checking cache for {} packages...",
                    dependencies.len()
                );
            }
            let to_generate = self.check_cache(&dependencies, &ament_packages)?;

            if to_generate.is_empty() {
                break; // No more packages to generate
            }

            if self.verbose {
                eprintln!("  {} packages need generation", to_generate.len());
            }

            // Generate bindings (in parallel if multiple packages)
            let generated_packages = if to_generate.len() > 1 {
                self.generate_bindings_parallel(&to_generate, &ament_packages)?
            } else {
                let mut generated_packages = Vec::new();
                for package_name in &to_generate {
                    let output_dir = self.generate_bindings(package_name)?;

                    // Get share dir for checksum calculation
                    if let Some(share_dir) = ament_packages.get(package_name) {
                        self.update_cache(package_name, share_dir, output_dir.clone())?;
                    }

                    generated_packages.push((package_name.clone(), output_dir));
                }
                generated_packages
            };

            all_generated.extend(generated_packages.clone());

            // Mark these packages as seen
            for (pkg_name, _) in &generated_packages {
                seen_packages.insert(pkg_name.clone());
            }

            // Discover transitive dependencies from generated packages
            let mut new_deps = Vec::new();
            for (_pkg_name, pkg_path) in &generated_packages {
                if let Ok(transitive_deps) = self.discover_transitive_dependencies(pkg_path) {
                    for dep in transitive_deps {
                        // Only add if we haven't seen it yet and it's a known ROS package
                        if !seen_packages.contains(&dep) && ament_packages.contains_key(&dep) {
                            new_deps.push(RosDependency {
                                name: dep.clone(),
                                direct: false,
                            });
                            seen_packages.insert(dep);
                        }
                    }
                }
            }

            if new_deps.is_empty() {
                break; // No new dependencies found
            }

            if self.verbose {
                eprintln!("  Discovered {} transitive dependencies", new_deps.len());
            }

            dependencies = new_deps;
        }

        // Step 4: Patch .cargo/config.toml
        if !all_generated.is_empty() {
            if self.verbose {
                eprintln!("Step 4: Patching .cargo/config.toml...");
            }
            self.patch_cargo_config(&all_generated)?;
        }

        // Step 5: Invoke cargo build (unless --bindings-only)
        if !bindings_only {
            if self.verbose {
                eprintln!("Step 5: Invoking cargo build...");
            }
            self.invoke_cargo_build()?;
        }

        Ok(())
    }

    /// Discover transitive dependencies from a generated package
    fn discover_transitive_dependencies(&self, package_path: &Path) -> Result<Vec<String>> {
        use std::fs;

        let cargo_toml_path = package_path.join("Cargo.toml");
        let contents = fs::read_to_string(&cargo_toml_path)
            .wrap_err_with(|| format!("Failed to read {}", cargo_toml_path.display()))?;

        let mut dependencies = Vec::new();
        let mut in_dependencies = false;

        for line in contents.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("[dependencies]") {
                in_dependencies = true;
                continue;
            }

            if trimmed.starts_with('[') {
                in_dependencies = false;
                continue;
            }

            if in_dependencies && !trimmed.is_empty() && !trimmed.starts_with('#') {
                // Extract package name (before '=' or space)
                if let Some(pkg_name) = trimmed.split('=').next() {
                    let pkg_name = pkg_name.trim();
                    // Skip special entries like "serde" or "rosidl-runtime-rs"
                    if !pkg_name.contains('-') || pkg_name.contains('_') {
                        dependencies.push(pkg_name.to_string());
                    }
                }
            }
        }

        Ok(dependencies)
    }

    /// Invoke cargo build
    fn invoke_cargo_build(&self) -> Result<()> {
        if self.verbose {
            eprintln!("Step 4: Invoking cargo build...");
        }

        let status = Command::new("cargo")
            .arg("build")
            .current_dir(&self.project_root)
            .status()
            .wrap_err("Failed to execute cargo build")?;

        if !status.success() {
            return Err(eyre!("cargo build failed"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_context_new() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ctx = WorkflowContext::new(temp_dir.path().to_path_buf(), false);

        assert_eq!(ctx.project_root, temp_dir.path());
        assert_eq!(
            ctx.output_dir,
            temp_dir.path().join("target").join("ros2_bindings")
        );
        assert_eq!(ctx.cache_file, temp_dir.path().join(CACHE_FILE_NAME));
        assert!(!ctx.verbose);
    }

    #[test]
    fn test_workflow_context_verbose() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ctx = WorkflowContext::new(temp_dir.path().to_path_buf(), true);

        assert!(ctx.verbose);
    }

    #[test]
    fn test_discover_ament_packages_no_ros() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ctx = WorkflowContext::new(temp_dir.path().to_path_buf(), false);

        // If ROS is not sourced, this will fail
        // If ROS is sourced, it should return packages
        let result = ctx.discover_ament_packages();

        // Either way is fine for this test - we're just checking it doesn't panic
        match result {
            Ok(packages) => {
                // ROS is sourced - packages may or may not be empty
                eprintln!("Found {} ROS packages", packages.len());
            }
            Err(e) => {
                // ROS is not sourced - expected
                // The error should mention the environment variable issue
                let error_str = e.to_string();
                assert!(
                    error_str.contains("AMENT_PREFIX_PATH")
                        || error_str.contains("environment variable not set")
                        || error_str.contains("Failed to load ament index"),
                    "Unexpected error message: {}",
                    error_str
                );
            }
        }
    }
}
