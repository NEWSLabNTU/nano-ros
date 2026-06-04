# Licensed under the Apache License, Version 2.0

import os
from pathlib import Path

from colcon_core.environment import create_environment_hooks, create_environment_scripts
from colcon_core.logging import colcon_logger
from colcon_core.plugin_system import satisfies_version
from colcon_core.shell import create_environment_hook
from colcon_core.task import TaskExtensionPoint, run

# Import Rust library directly via PyO3 bindings
from colcon_nano_ros import cargo_ros2_py
from colcon_nano_ros.workspace_bindgen import generate_workspace_bindings

logger = colcon_logger.getChild(__name__)


class AmentCargoBuildTask(TaskExtensionPoint):
    """A build task for Rust ROS 2 packages using workspace-level binding generation.

    This task implements a two-phase approach:
    1. Workspace-level binding generation (done once before all builds)
    2. Per-package cargo build with --config flag

    The workspace-level binding generation:
    - Discovers all ROS dependencies from ament_index and workspace
    - Generates ALL bindings to build/ros2_bindings/
    - Writes single Cargo config file to build/ros2_cargo_config.toml
    - Uses lock file to ensure only one process does generation

    Each package build then runs:
    - cargo build --config build/ros2_cargo_config.toml

    This eliminates race conditions, improves build performance, and avoids
    conflicts with user's own .cargo/config.toml files.
    """

    def __init__(self):  # noqa: D107
        super().__init__()
        satisfies_version(TaskExtensionPoint.EXTENSION_POINT_VERSION, "^1.0")
        self._build_base = None  # Will be set during workspace binding generation

    def add_arguments(self, *, parser):  # noqa: D102
        parser.add_argument(
            "--cargo-args",
            nargs="*",
            metavar="*",
            type=str.lstrip,
            help="Pass arguments to Cargo. "
            "Arguments matching other options must be prefixed by a space,\n"
            'e.g. --cargo-args " --help"',
        )

    async def build(self, *, additional_hooks=None):  # noqa: D102
        """Build the Rust ROS 2 package using workspace-level binding generation."""
        additional_hooks = [] if additional_hooks is None else additional_hooks

        # Step 1: Generate workspace-level bindings (done once for entire workspace)
        rc = await self._prepare_workspace_bindings()
        if rc:
            return rc

        # Step 2: Create environment hooks and scripts
        await self._create_environment_scripts(additional_hooks)

        # Step 3: Build this package with cargo
        args = self.context.args
        cmd = self._build_cmd(args.cargo_args if hasattr(args, "cargo_args") else [])

        # Execute cargo build from workspace root (so relative paths in config work)
        result = await run(self.context, cmd, cwd=self._workspace_root, env=None)
        if result and result.returncode != 0:
            return result.returncode

        # Step 4: Install binaries and create package markers
        rc = self._install_package()
        if rc:
            return rc

        # Return the exit code
        return 0

    async def _prepare_workspace_bindings(self):
        """Generate workspace-level ROS 2 bindings (done once for entire workspace)."""
        # Check that cargo_ros2_py module is available
        try:
            # Quick check that the module loaded correctly
            _ = cargo_ros2_py.__version__
            logger.debug(f"cargo_ros2_py {cargo_ros2_py.__version__} loaded")
        except (ImportError, AttributeError) as e:
            logger.error(
                f"\n\ncargo_ros2_py Rust bindings not found: {e}"
                "\n\nPlease ensure colcon-cargo-ros2 is installed correctly:"
                "\n  $ pip install colcon-cargo-ros2\n"
            )
            return 1

        # Derive workspace paths from install_base
        args = self.context.args
        workspace_root = Path(os.path.abspath(os.path.join(args.install_base, "../..")))
        build_base = Path(os.path.abspath(os.path.join(args.build_base, "..")))
        install_base = Path(args.install_base).parent  # install/ directory

        # Store paths for use in build
        self._workspace_root = workspace_root
        self._build_base = build_base

        # Generate workspace-level bindings
        # This uses a lock file, so only the first package will actually generate
        # All other packages will see the lock and skip generation
        try:
            verbose = getattr(args, "verbose", False)
            generate_workspace_bindings(workspace_root, build_base, install_base, args, verbose)
        except Exception as e:
            logger.error(f"Workspace binding generation failed: {e}")
            import traceback

            logger.error(traceback.format_exc())
            return 1

        return 0

    async def _create_environment_scripts(self, additional_hooks):
        """Create environment hooks and scripts for ROS 2 integration.

        This creates:
        1. Individual hook scripts (e.g., ament_prefix_path.sh)
        2. Package scripts that source all hooks (package.sh, package.bash, etc.)
        3. Ensures ROS 2 compliance so CMake packages can find our packages
        """
        args = self.context.args
        pkg = self.context.pkg

        # Create additional hooks (e.g., ament_prefix_path)
        additional_hooks.extend(
            create_environment_hook(
                "ament_prefix_path",
                Path(args.install_base),
                pkg.name,
                "AMENT_PREFIX_PATH",
                "",
                mode="prepend",
            )
        )

        # Create default environment hooks (PATH, PYTHONPATH, etc.) from environment extensions
        default_hooks = create_environment_hooks(args.install_base, pkg.name)

        # Create package scripts (package.sh, package.bash, etc.) that source all hooks
        # This is what makes our Rust packages compatible with CMake packages
        create_environment_scripts(
            pkg, args, default_hooks=default_hooks, additional_hooks=additional_hooks
        )

    def _build_cmd(self, cargo_args):
        """Build the cargo build command.

        Since bindings are generated at workspace-level, we pass --config flag
        to use the single config file in build/ros2_cargo_config.toml.
        We also use --manifest-path since cargo is invoked from workspace root.

        Adds --quiet flag by default to suppress cargo progress output (matching
        CMake/Python build behavior), unless verbose mode is enabled.
        """
        cmd = ["cargo", "build"]

        # Add --manifest-path to specify which package to build
        # (since we're running cargo from workspace root, not package dir)
        manifest_path = Path(self.context.pkg.path) / "Cargo.toml"
        cmd.extend(["--manifest-path", str(manifest_path)])

        # Add --config flag to use workspace-level config file
        if self._build_base:
            config_file = self._build_base / "ros2_cargo_config.toml"
            cmd.extend(["--config", str(config_file)])

        # Handle None cargo_args
        if cargo_args is None:
            cargo_args = []

        # Add --quiet flag unless verbose mode is enabled or user explicitly passed --verbose
        # This suppresses "Compiling..." and "Finished..." messages but shows errors
        args = self.context.args
        verbose = getattr(args, "verbose", False)
        has_verbose_flag = "--verbose" in cargo_args or "-v" in cargo_args
        has_quiet_flag = "--quiet" in cargo_args or "-q" in cargo_args

        if not verbose and not has_verbose_flag and not has_quiet_flag:
            cmd.append("--quiet")

        # Add all cargo arguments
        cmd.extend(cargo_args)

        return cmd

    def _detect_cargo_profile(self, cargo_args, args):
        """Detect the cargo build profile from command-line arguments.

        Supports:
        - --release flag → "release"
        - --profile NAME → NAME (custom profile)
        - --profile=NAME → NAME (custom profile)
        - dev profile → "debug" (special case: dev outputs to target/debug/)
        - default → "debug"
        """
        # Check colcon-level --release flag first
        if hasattr(args, "release") and args.release:
            return "release"

        # Parse cargo arguments
        i = 0
        while i < len(cargo_args):
            arg = cargo_args[i]

            # Check for --release flag
            if arg == "--release":
                return "release"

            # Check for --profile=NAME syntax
            if arg.startswith("--profile="):
                profile_name = arg.split("=", 1)[1]
                # Special case: dev profile outputs to debug directory
                return "debug" if profile_name == "dev" else profile_name

            # Check for --profile NAME syntax (two separate args)
            if arg == "--profile" and i + 1 < len(cargo_args):
                profile_name = cargo_args[i + 1]
                # Special case: dev profile outputs to debug directory
                return "debug" if profile_name == "dev" else profile_name

            i += 1

        # Default to debug (dev profile)
        return "debug"

    def _install_package(self):
        """Install package binaries and create ament markers using direct API call."""
        args = self.context.args

        # Determine build profile from cargo arguments
        # Supports: --release, --profile NAME, --profile=NAME
        cargo_args = getattr(args, "cargo_args", []) or []
        profile = self._detect_cargo_profile(cargo_args, args)
        verbose = getattr(args, "verbose", False)

        # Execute installation via direct API call
        try:
            # Create configuration for installation
            # Ensure project_root is an absolute path
            project_root = Path(self.context.pkg.path).resolve()

            config = cargo_ros2_py.InstallConfig(
                project_root=str(project_root),
                install_base=str(args.install_base),
                build_base=str(self._build_base),
                profile=profile,
                verbose=verbose,
            )

            # Call Rust function directly (no subprocess!)
            cargo_ros2_py.install_to_ament(config)

            logger.info("✓ Package installed successfully")
            return 0

        except RuntimeError as e:
            logger.error(f"Installation failed: {e}")
            return 1
