# Licensed under the Apache License, Version 2.0

"""Workspace-level ROS 2 binding generation for Rust.

This module provides centralized binding generation for an entire colcon workspace.
Instead of each package generating bindings independently (causing race conditions),
this module generates ALL bindings once before any packages are built.

Architecture:
1. Discover all ROS package dependencies in the workspace
2. Generate all bindings to build/<pkg>/rosidl_cargo/
3. Write single Cargo config file in build/ros2_cargo_config.toml
4. Individual packages run `cargo build --config build/ros2_cargo_config.toml`
"""

from pathlib import Path
from typing import Dict

from colcon_core.logging import colcon_logger

# Import Rust library directly via PyO3 bindings
from colcon_nano_ros import cargo_ros2_py

logger = colcon_logger.getChild(__name__)


class WorkspaceBindingGenerator:
    """Generates ROS 2 Rust bindings for an entire colcon workspace."""

    def __init__(self, workspace_root: Path, build_base: Path, install_base: Path, args):
        """Initialize the workspace binding generator.

        Args:
            workspace_root: Root directory of the colcon workspace
            build_base: Base directory for build artifacts (workspace/build/)
            install_base: Base directory for installed packages (workspace/install/)
            args: Colcon command line arguments
        """
        self.workspace_root = workspace_root
        self.build_base = build_base
        self.install_base = install_base
        self.args = args
        self.lock_file = build_base / ".colcon" / "bindgen.lock"

    def should_generate(self) -> bool:
        """Check if binding generation is needed (not already done by another process)."""
        # If lock file exists, another process is/was handling binding generation
        if self.lock_file.exists():
            logger.info(f"Binding generation lock exists: {self.lock_file}")
            return False

        # Create lock file to indicate we're handling binding generation
        self.lock_file.parent.mkdir(parents=True, exist_ok=True)
        self.lock_file.write_text("locked")
        return True

    def generate_all_bindings(self, verbose: bool = False):
        """Generate all ROS 2 bindings for the workspace.

        This is the main entry point that:
        1. Discovers all ROS dependencies
        2. Generates bindings for all packages
        3. Writes single Cargo config file in build/
        """
        logger.info("Starting workspace-level binding generation")

        # Step 1: Discover all ROS dependencies from ament_index and workspace
        ros_packages = self._discover_ros_packages()
        logger.info(f"Discovered {len(ros_packages)} ROS packages")

        # Step 1.5: Validate Cargo.toml dependencies match package.xml
        self._validate_cargo_dependencies(ros_packages)

        # Step 2: Generate bindings for all discovered packages
        self._generate_bindings(ros_packages, verbose)

        # Step 3: Write single Cargo config file in build/ directory
        self._write_cargo_config_file(ros_packages)

        logger.info("Workspace-level binding generation complete")

    def _discover_ros_packages(self) -> Dict[str, Path]:
        """Discover ROS interface packages that are dependencies of workspace Cargo packages.

        This implements dependency-aware binding generation:
        1. Get Cargo packages from augmentation (with parsed dependencies from package.xml)
        2. Extract direct ROS dependencies from Colcon
        3. Resolve transitive dependencies using catkin_pkg
        4. Filter to only interface packages (have msg/srv/action)

        Returns:
            Dict mapping package names to their share/ directory paths
        """
        from colcon_nano_ros.package_augmentation import RustBindingAugmentation

        # Get Cargo package descriptors (includes parsed dependencies from package.xml)
        cargo_descriptors = getattr(RustBindingAugmentation, "_cargo_descriptors", {})

        if not cargo_descriptors:
            logger.info("No Cargo packages found in workspace")
            return {}

        logger.info(f"Discovering dependencies for {len(cargo_descriptors)} Cargo packages")

        # Step 1: Get direct ROS dependencies from Colcon-parsed package.xml
        required_packages = set()

        for pkg_name, desc in cargo_descriptors.items():
            # Get build + run dependencies (interface packages needed at compile time)
            # desc.dependencies is populated by Colcon's RosPackageIdentification
            # from package.xml using catkin_pkg
            deps = desc.get_dependencies(categories=["build", "run"])
            dep_names = [d.name for d in deps]
            required_packages.update(dep_names)

            if dep_names:
                logger.info(f"{pkg_name} has {len(dep_names)} direct dependencies: {dep_names}")

        logger.info(f"Total direct dependencies: {len(required_packages)}")

        # Step 2: Resolve transitive dependencies using catkin_pkg
        # This handles: my_pkg -> geometry_msgs -> std_msgs -> builtin_interfaces
        required_packages = self._resolve_transitive_dependencies(required_packages)

        logger.info(f"Total after transitive resolution: {len(required_packages)}")

        # Step 3: Check workspace packages for interfaces (from source directories)
        # This also discovers their dependencies
        workspace_interface_packages, workspace_deps = self._find_workspace_interface_packages(
            required_packages
        )

        # Add dependencies of workspace packages to required set
        required_packages.update(workspace_deps)

        # Re-resolve transitive dependencies including workspace package dependencies
        if workspace_deps:
            logger.info(f"Adding {len(workspace_deps)} dependencies from workspace packages")
            required_packages = self._resolve_transitive_dependencies(required_packages)
            logger.info(
                f"Total after resolving workspace package dependencies: {len(required_packages)}"
            )

        # Step 4: Filter remaining packages to interface packages (from ament_index)
        remaining_packages = required_packages - set(workspace_interface_packages.keys())
        interface_packages = self._filter_interface_packages(remaining_packages)

        # Merge workspace and system interface packages
        interface_packages.update(workspace_interface_packages)

        logger.info(f"Final interface packages to generate: {len(interface_packages)}")

        return interface_packages

    def _validate_cargo_dependencies(self, interface_packages: Dict[str, Path]):
        """Validate that Cargo.toml dependencies match package.xml interface packages.

        Prints warnings if there are mismatches between what's declared in package.xml
        and what's actually used in Cargo.toml.

        Args:
            interface_packages: Dict of discovered interface packages from package.xml
        """
        from colcon_nano_ros.package_augmentation import RustBindingAugmentation

        cargo_descriptors = getattr(RustBindingAugmentation, "_cargo_descriptors", {})
        logger.debug(f"Validating Cargo.toml dependencies for {len(cargo_descriptors)} packages")

        for pkg_name, desc in cargo_descriptors.items():
            pkg_path = Path(desc.path)
            cargo_toml_path = pkg_path / "Cargo.toml"

            if not cargo_toml_path.exists():
                continue

            try:
                # Parse Cargo.toml to extract dependencies
                # Use tomllib (Python 3.11+) or tomli (Python 3.8-3.10)
                try:
                    import tomllib
                except ImportError:
                    import tomli as tomllib

                with open(cargo_toml_path, "rb") as f:
                    cargo_data = tomllib.load(f)

                # Get all dependencies from Cargo.toml (regular + build-dependencies)
                cargo_deps = set()
                if "dependencies" in cargo_data:
                    cargo_deps.update(cargo_data["dependencies"].keys())
                if "build-dependencies" in cargo_data:
                    cargo_deps.update(cargo_data["build-dependencies"].keys())

                # Get interface packages from package.xml
                xml_deps = desc.get_dependencies(categories=["build", "run"])
                xml_interface_deps = set(d.name for d in xml_deps if d.name in interface_packages)

                # Check for interface packages in package.xml but not in Cargo.toml
                missing_in_cargo = xml_interface_deps - cargo_deps
                if missing_in_cargo:
                    logger.warning(
                        f"{pkg_name}: Interface packages in package.xml but not in Cargo.toml: "
                        f"{', '.join(sorted(missing_in_cargo))}"
                    )

                # Check for ROS packages in Cargo.toml but not in package.xml
                # (Only check packages that we generated bindings for)
                extra_in_cargo = cargo_deps & set(interface_packages.keys()) - xml_interface_deps
                if extra_in_cargo:
                    logger.warning(
                        f"{pkg_name}: Interface packages in Cargo.toml but not in package.xml: "
                        f"{', '.join(sorted(extra_in_cargo))}. "
                        "Add them to package.xml with <depend> tags."
                    )

            except Exception as e:
                logger.debug(f"Could not validate Cargo.toml for {pkg_name}: {e}")

    def _find_workspace_interface_packages(self, required_packages: set):
        """Find interface packages in the workspace from source directories.

        This handles workspace-local packages that haven't been installed yet.
        Also discovers their dependencies to ensure complete binding generation.

        Args:
            required_packages: Set of package names to check

        Returns:
            Tuple of (workspace_interface_packages, workspace_dependencies):
            - workspace_interface_packages: Dict mapping package names to paths
            - workspace_dependencies: Set of dependency names from workspace packages
        """
        from catkin_pkg.package import parse_package

        from colcon_nano_ros.package_augmentation import RustBindingAugmentation

        workspace_interface_packages = {}
        workspace_dependencies = set()

        # Get all package descriptors discovered by colcon
        all_descriptors = getattr(RustBindingAugmentation, "_all_descriptors", set())

        # Create a mapping of package name -> descriptor
        descriptors_by_name = {desc.name: desc for desc in all_descriptors}

        for pkg_name in required_packages:
            if pkg_name in descriptors_by_name:
                desc = descriptors_by_name[pkg_name]
                pkg_path = Path(desc.path)

                # Check if package has interface definitions in source directory
                has_interfaces = any(
                    [
                        (pkg_path / "msg").exists(),
                        (pkg_path / "srv").exists(),
                        (pkg_path / "action").exists(),
                    ]
                )

                if has_interfaces:
                    # For workspace packages, we use the source directory as the "share" path
                    # because the package hasn't been installed yet
                    workspace_interface_packages[pkg_name] = pkg_path
                    logger.info(f"Found workspace interface package: {pkg_name} at {pkg_path}")

                    # Parse package.xml to discover dependencies of workspace package
                    try:
                        pkg = parse_package(str(pkg_path))
                        import os

                        condition_context = {**os.environ}
                        pkg.evaluate_conditions(condition_context)

                        # Get all build + run dependencies
                        deps = set()
                        for d in pkg.build_depends:
                            if d.evaluated_condition:
                                deps.add(d.name)
                        for d in pkg.build_export_depends:
                            if d.evaluated_condition:
                                deps.add(d.name)
                        for d in pkg.exec_depends:
                            if d.evaluated_condition:
                                deps.add(d.name)

                        if deps:
                            logger.debug(f"{pkg_name} (workspace) added deps: {deps}")
                            workspace_dependencies.update(deps)

                    except Exception as e:
                        logger.debug(
                            f"Could not parse package.xml for workspace package {pkg_name}: {e}"
                        )

        return workspace_interface_packages, workspace_dependencies

    def _resolve_transitive_dependencies(self, initial_packages: set) -> set:
        """Resolve transitive ROS dependencies using catkin_pkg.

        This is the official ROS 2 method for parsing package.xml files.
        Despite the name, catkin_pkg is used by ROS 2 (see colcon-ros documentation).

        Args:
            initial_packages: Set of direct dependency package names

        Returns:
            Set of all packages (direct + transitive)
        """
        import os

        from ament_index_python.packages import get_package_share_directory
        from catkin_pkg.package import parse_package

        # Add workspace install directory to AMENT_PREFIX_PATH so we can find
        # workspace-local packages
        original_ament_prefix = os.environ.get("AMENT_PREFIX_PATH", "")
        if self.install_base.exists():
            if original_ament_prefix:
                os.environ["AMENT_PREFIX_PATH"] = f"{self.install_base}:{original_ament_prefix}"
            else:
                os.environ["AMENT_PREFIX_PATH"] = str(self.install_base)

        all_packages = set(initial_packages)
        visited = set()
        queue = set(initial_packages)

        while queue:
            pkg_name = queue.pop()
            if pkg_name in visited:
                continue
            visited.add(pkg_name)

            try:
                # Get package share directory from ament_index
                pkg_share = Path(get_package_share_directory(pkg_name))

                # Parse package.xml using catkin_pkg (official ROS 2 method)
                pkg = parse_package(str(pkg_share))

                # Evaluate conditional dependencies (ROS_VERSION, etc.)
                # This is required - evaluated_condition is None before this call
                import os

                condition_context = {**os.environ}
                pkg.evaluate_conditions(condition_context)

                # Get all build + run dependencies (matching Colcon's logic)
                # This follows RosPackageIdentification in colcon-ros
                deps = set()

                # Add build dependencies
                for d in pkg.build_depends:
                    if d.evaluated_condition:  # Respect conditional dependencies
                        deps.add(d.name)

                # Add build export dependencies (transitive build deps)
                for d in pkg.build_export_depends:
                    if d.evaluated_condition:
                        deps.add(d.name)

                # Add exec dependencies (runtime deps)
                for d in pkg.exec_depends:
                    if d.evaluated_condition:
                        deps.add(d.name)

                # Add new dependencies to the queue
                new_deps = deps - visited
                if new_deps:
                    logger.debug(f"{pkg_name} added transitive deps: {new_deps}")
                    queue.update(new_deps)
                    all_packages.update(new_deps)

            except Exception as e:
                logger.debug(f"Could not resolve dependencies for {pkg_name}: {e}")

        # Restore original AMENT_PREFIX_PATH
        if original_ament_prefix:
            os.environ["AMENT_PREFIX_PATH"] = original_ament_prefix
        elif "AMENT_PREFIX_PATH" in os.environ:
            del os.environ["AMENT_PREFIX_PATH"]

        return all_packages

    def _filter_interface_packages(self, packages: set) -> Dict[str, Path]:
        """Filter packages to only those with msg/srv/action interfaces.

        Args:
            packages: Set of package names

        Returns:
            Dict mapping interface package names to their share/ directory paths
        """
        import os

        from ament_index_python.packages import get_package_share_directory

        # Add workspace install directory to AMENT_PREFIX_PATH so we can find
        # workspace-local packages
        original_ament_prefix = os.environ.get("AMENT_PREFIX_PATH", "")
        if self.install_base.exists():
            if original_ament_prefix:
                os.environ["AMENT_PREFIX_PATH"] = f"{self.install_base}:{original_ament_prefix}"
            else:
                os.environ["AMENT_PREFIX_PATH"] = str(self.install_base)

        interface_packages = {}

        for pkg_name in packages:
            try:
                pkg_share = Path(get_package_share_directory(pkg_name))

                # Check if package has interface definitions
                has_interfaces = any(
                    [
                        (pkg_share / "msg").exists(),
                        (pkg_share / "srv").exists(),
                        (pkg_share / "action").exists(),
                    ]
                )

                if has_interfaces:
                    interface_packages[pkg_name] = pkg_share
                    logger.debug(f"Interface package: {pkg_name}")
                else:
                    logger.debug(f"Skipping non-interface package: {pkg_name}")

            except Exception as e:
                logger.debug(f"Could not check {pkg_name}: {e}")

        # Restore original AMENT_PREFIX_PATH
        if original_ament_prefix:
            os.environ["AMENT_PREFIX_PATH"] = original_ament_prefix
        elif "AMENT_PREFIX_PATH" in os.environ:
            del os.environ["AMENT_PREFIX_PATH"]

        return interface_packages

    def _generate_bindings(self, ros_packages: Dict[str, Path], verbose: bool):
        """Generate Rust bindings for all ROS packages.

        Each package's bindings are generated to build/<pkg_name>/rosidl_cargo/

        Args:
            ros_packages: Dict mapping package names to share/ directories
            verbose: Enable verbose output
        """
        # Generate bindings for each package that has interfaces
        for pkg_name, pkg_share in ros_packages.items():
            # Check if package has interfaces (msg/, srv/, action/ directories)
            has_interfaces = any(
                [
                    (pkg_share / "msg").exists(),
                    (pkg_share / "srv").exists(),
                    (pkg_share / "action").exists(),
                ]
            )

            if not has_interfaces:
                continue

            # Output directory: build/<pkg_name>/rosidl_cargo/
            pkg_build_dir = self.build_base / pkg_name / "rosidl_cargo"

            # Check if bindings already exist and are up-to-date
            # Generated structure is: build/<pkg_name>/rosidl_cargo/<pkg_name>/Cargo.toml
            binding_dir = pkg_build_dir / pkg_name
            if binding_dir.exists():
                # TODO: Add checksum-based cache validation
                logger.debug(f"Bindings already exist for {pkg_name}")
                continue

            # Generate bindings using cargo ros2 bindgen
            logger.info(f"Generating bindings for {pkg_name}")
            try:
                self._run_bindgen(pkg_name, pkg_share, pkg_build_dir, verbose)
                # Post-process generated Cargo.toml to remove path dependencies
                # NOTE: This only modifies GENERATED bindings, not user's Cargo.toml
                self._fixup_generated_cargo_toml(pkg_name, binding_dir)
            except RuntimeError as e:
                # Log warning for packages that can't be generated (e.g., unsupported IDL features)
                logger.warning(f"Skipping {pkg_name}: {e}")

    def _run_bindgen(self, pkg_name: str, pkg_share: Path, output_dir: Path, verbose: bool):
        """Generate Rust bindings for a single package using direct API call.

        Args:
            pkg_name: Name of the ROS package
            pkg_share: Path to the package's share/ directory
            output_dir: Path where bindings should be generated
            verbose: Enable verbose output
        """
        try:
            # Create configuration for binding generation
            config = cargo_ros2_py.BindgenConfig(
                package_name=pkg_name,
                output_dir=str(output_dir),
                package_path=str(pkg_share),
                verbose=verbose,
            )

            # Call Rust function directly (no subprocess!)
            cargo_ros2_py.generate_bindings(config)

            if verbose:
                logger.info(f"✓ Generated bindings for {pkg_name}")

        except RuntimeError as e:
            logger.error(f"Failed to generate bindings for {pkg_name}: {e}")
            raise

    def _fixup_generated_cargo_toml(self, pkg_name: str, binding_dir: Path):
        """Post-process GENERATED Cargo.toml to convert path dependencies to version requirements.

        This is necessary because rosidl-bindgen generates Cargo.toml with local
        path dependencies (e.g., `std_msgs = { path = "../std_msgs" }`), but we want
        to use the .cargo/config.toml patches instead.

        NOTE: This ONLY modifies generated binding Cargo.toml files, NOT user's Cargo.toml files.
        Users are responsible for maintaining their own Cargo.toml dependencies.

        Args:
            pkg_name: Name of the ROS package
            binding_dir: Directory containing the generated bindings
        """
        # Find the Cargo.toml (nested structure: binding_dir/pkg_name/Cargo.toml)
        cargo_toml = binding_dir / pkg_name / "Cargo.toml"
        if not cargo_toml.exists():
            # Try top-level
            cargo_toml = binding_dir / "Cargo.toml"
            if not cargo_toml.exists():
                # This is expected for packages without interfaces (msg/srv/action)
                logger.debug(f"No Cargo.toml found for {pkg_name} (package has no interfaces)")
                return

        # Read the Cargo.toml
        content = cargo_toml.read_text()
        lines = content.split("\n")

        # Process each line to convert path dependencies to version requirements
        new_lines = []
        in_dependencies = False
        for line in lines:
            # Track when we're in [dependencies] or [build-dependencies] section
            if line.strip().startswith("[dependencies]") or line.strip().startswith(
                "[build-dependencies]"
            ):
                in_dependencies = True
                new_lines.append(line)
                continue
            elif line.strip().startswith("[") and in_dependencies:
                in_dependencies = False
                new_lines.append(line)
                continue

            # If we're in dependencies section and line has a path dependency, convert it
            if in_dependencies and "{ path =" in line:
                # Extract package name from line like: `std_msgs = { path = "../std_msgs" }`
                if "=" in line:
                    dep_name = line.split("=")[0].strip()
                    # Convert all path dependencies to version requirements
                    # including rosidl_runtime_rs (will be patched to shared location)
                    new_lines.append(f'{dep_name} = "*"')
                    continue

            new_lines.append(line)

        # Write back the modified Cargo.toml
        cargo_toml.write_text("\n".join(new_lines))
        logger.debug(f"Fixed up generated Cargo.toml for {pkg_name}")

    def _write_cargo_config_file(self, ros_packages: Dict[str, Path]):
        """Write single Cargo config file in build/ directory with relative paths.

        This config file will be passed to cargo via --config flag.
        Paths are relative to the workspace root for portability.

        Args:
            ros_packages: Dict of all ROS packages (for building patch entries)
        """
        config_file = self.build_base / "ros2_cargo_config.toml"

        # Build [patch.crates-io] section
        patches = []

        for pkg_name in sorted(ros_packages.keys()):
            # Check per-package location: build/<pkg_name>/rosidl_cargo/
            pkg_build_dir = self.build_base / pkg_name / "rosidl_cargo"

            if pkg_build_dir.exists():
                # rosidl-bindgen creates nested structure: <pkg_name>/<pkg_name>/Cargo.toml
                nested_pkg_dir = pkg_build_dir / pkg_name
                if nested_pkg_dir.exists() and (nested_pkg_dir / "Cargo.toml").exists():
                    # Use relative path from workspace root for portability
                    rel_path = nested_pkg_dir.relative_to(self.workspace_root)
                    patches.append(f'{pkg_name} = {{ path = "{rel_path}" }}')
                elif (pkg_build_dir / "Cargo.toml").exists():
                    # Use the top-level directory if Cargo.toml is there
                    rel_path = pkg_build_dir.relative_to(self.workspace_root)
                    patches.append(f'{pkg_name} = {{ path = "{rel_path}" }}')

        # Build [build] section with rustflags for linker search paths
        # This is critical for finding workspace-local ROS package libraries
        rustflags = []

        # Add workspace install directory lib paths
        if self.install_base.exists():
            for pkg_install in self.install_base.iterdir():
                if not pkg_install.is_dir():
                    continue
                lib_dir = pkg_install / "lib"
                if lib_dir.exists():
                    rustflags.append(f'"-L", "native={lib_dir.absolute()}"')

        # Add system ROS library paths from AMENT_PREFIX_PATH
        import os

        if "AMENT_PREFIX_PATH" in os.environ:
            for prefix in os.environ["AMENT_PREFIX_PATH"].split(":"):
                lib_path = Path(prefix) / "lib"
                if lib_path.exists():
                    rustflags.append(f'"-L", "native={lib_path.absolute()}"')

        # Write config.toml
        content = "[patch.crates-io]\n"
        content += "\n".join(patches)
        content += "\n"

        if rustflags:
            content += "\n[build]\n"
            content += "rustflags = [\n"
            content += ",\n".join(f"    {flag}" for flag in rustflags)
            content += "\n]\n"

        config_file.write_text(content)
        logger.info(f"Wrote Cargo config with {len(patches)} patches to {config_file}")


def generate_workspace_bindings(
    workspace_root: Path,
    build_base: Path,
    install_base: Path,
    args,
    verbose: bool = False,
):
    """Generate bindings for an entire workspace (convenience function).

    Args:
        workspace_root: Root directory of the colcon workspace
        build_base: Base directory for build artifacts
        install_base: Base directory for installed packages
        args: Colcon command line arguments
        verbose: Enable verbose output
    """
    generator = WorkspaceBindingGenerator(workspace_root, build_base, install_base, args)

    # Only generate if we're the first process to get the lock
    if generator.should_generate():
        generator.generate_all_bindings(verbose)
    else:
        logger.info("Binding generation already handled by another process")
