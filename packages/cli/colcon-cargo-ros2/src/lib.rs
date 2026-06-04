//! Python bindings for cargo-nano-ros
//!
//! This module exposes cargo-nano-ros's functionality to Python using PyO3.

#![allow(clippy::useless_conversion)] // False positive with PyO3 macros
#![allow(unsafe_op_in_unsafe_fn)] // PyO3 macros generate unsafe ops without blocks

use pyo3::{exceptions::PyRuntimeError, prelude::*};
use std::path::PathBuf;

/// Python wrapper for BindgenConfig
#[pyclass]
#[derive(Clone)]
struct BindgenConfig {
    #[pyo3(get, set)]
    package_name: String,
    #[pyo3(get, set)]
    package_path: Option<String>,
    #[pyo3(get, set)]
    output_dir: String,
    #[pyo3(get, set)]
    verbose: bool,
}

#[pymethods]
impl BindgenConfig {
    #[new]
    #[pyo3(signature = (package_name, output_dir, package_path=None, verbose=false))]
    fn new(
        package_name: String,
        output_dir: String,
        package_path: Option<String>,
        verbose: bool,
    ) -> Self {
        Self {
            package_name,
            package_path,
            output_dir,
            verbose,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "BindgenConfig(package_name='{}', output_dir='{}', package_path={:?}, verbose={})",
            self.package_name, self.output_dir, self.package_path, self.verbose
        )
    }
}

/// Python wrapper for InstallConfig
#[pyclass]
#[derive(Clone)]
struct InstallConfig {
    #[pyo3(get, set)]
    project_root: String,
    #[pyo3(get, set)]
    install_base: String,
    #[pyo3(get, set)]
    build_base: String,
    #[pyo3(get, set)]
    profile: String,
    #[pyo3(get, set)]
    verbose: bool,
}

#[pymethods]
impl InstallConfig {
    #[new]
    #[pyo3(signature = (project_root, install_base, build_base, profile="debug".to_string(), verbose=false))]
    fn new(
        project_root: String,
        install_base: String,
        build_base: String,
        profile: String,
        verbose: bool,
    ) -> Self {
        Self {
            project_root,
            install_base,
            build_base,
            profile,
            verbose,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "InstallConfig(project_root='{}', install_base='{}', build_base='{}', profile='{}', verbose={})",
            self.project_root, self.install_base, self.build_base, self.profile, self.verbose
        )
    }
}

/// Generate Rust bindings for a ROS 2 interface package
///
/// Args:
///     config (BindgenConfig): Configuration for binding generation
///
/// Raises:
///     RuntimeError: If binding generation fails
///
/// Example:
///     >>> import cargo_ros2_py
///     >>> config = cargo_ros2_py.BindgenConfig(
///     ...     package_name="std_msgs",
///     ...     output_dir="target/ros2_bindings",
///     ...     package_path=None,
///     ...     verbose=True
///     ... )
///     >>> cargo_ros2_py.generate_bindings(config)
#[pyfunction]
fn generate_bindings(config: BindgenConfig) -> PyResult<()> {
    let rust_config = cargo_nano_ros::BindgenConfig {
        package_name: config.package_name,
        package_path: config.package_path.map(PathBuf::from),
        output_dir: PathBuf::from(config.output_dir),
        verbose: config.verbose,
    };

    cargo_nano_ros::generate_bindings(rust_config)
        .map_err(|e| PyRuntimeError::new_err(format!("Binding generation failed: {:#}", e)))?;
    Ok(())
}

/// Install package binaries and libraries to ament layout
///
/// Args:
///     config (InstallConfig): Configuration for installation
///
/// Raises:
///     RuntimeError: If installation fails
///
/// Example:
///     >>> import cargo_ros2_py
///     >>> config = cargo_ros2_py.InstallConfig(
///     ...     project_root="/path/to/project",
///     ...     install_base="install/my_package",
///     ...     profile="release",
///     ...     verbose=True
///     ... )
///     >>> cargo_ros2_py.install_to_ament(config)
#[pyfunction]
fn install_to_ament(config: InstallConfig) -> PyResult<()> {
    let rust_config = cargo_nano_ros::InstallConfig {
        project_root: PathBuf::from(config.project_root),
        install_base: PathBuf::from(config.install_base),
        build_base: PathBuf::from(config.build_base),
        profile: config.profile,
        verbose: config.verbose,
    };

    cargo_nano_ros::install_to_ament(rust_config)
        .map_err(|e| PyRuntimeError::new_err(format!("Installation failed: {:#}", e)))?;
    Ok(())
}

/// Clean generated bindings and cache
///
/// Args:
///     project_root (str): Project root directory
///     verbose (bool): Enable verbose output
///
/// Raises:
///     RuntimeError: If cleanup fails
///
/// Example:
///     >>> import cargo_ros2_py
///     >>> cargo_ros2_py.clean_bindings("/path/to/project", verbose=True)
#[pyfunction]
fn clean_bindings(project_root: String, verbose: bool) -> PyResult<()> {
    cargo_nano_ros::clean_bindings(&PathBuf::from(project_root), verbose)
        .map_err(|e| PyRuntimeError::new_err(format!("Clean failed: {:#}", e)))?;
    Ok(())
}

/// Python module for cargo-nano-ros
///
/// This module provides Python bindings to the cargo-nano-ros library,
/// enabling direct function calls from Python without subprocess overhead.
#[pymodule]
fn cargo_ros2_py(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Register classes
    m.add_class::<BindgenConfig>()?;
    m.add_class::<InstallConfig>()?;

    // Register functions
    m.add_function(wrap_pyfunction!(generate_bindings, m)?)?;
    m.add_function(wrap_pyfunction!(install_to_ament, m)?)?;
    m.add_function(wrap_pyfunction!(clean_bindings, m)?)?;

    // Module metadata
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add(
        "__doc__",
        "Python bindings for cargo-nano-ros - Build tool for nros projects",
    )?;

    Ok(())
}
