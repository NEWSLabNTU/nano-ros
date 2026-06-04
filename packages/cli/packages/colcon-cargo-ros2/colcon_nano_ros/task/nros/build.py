# Licensed under the Apache License, Version 2.0

import json
import os
import shutil
import subprocess
from pathlib import Path

from colcon_core.environment import create_environment_hooks, create_environment_scripts
from colcon_core.logging import colcon_logger
from colcon_core.plugin_system import satisfies_version
from colcon_core.shell import create_environment_hook
from colcon_core.task import TaskExtensionPoint, run

logger = colcon_logger.getChild(__name__)

# Platform → Rust target triple mapping.
# None means native (no --target flag).
PLATFORM_TARGETS = {
    "native": None,
    "freertos": "thumbv7m-none-eabi",
    "baremetal": "thumbv7m-none-eabi",
    "nuttx": "thumbv7m-none-eabi",
    "threadx": "thumbv7m-none-eabi",  # TODO: also riscv64gc-unknown-none-elf
    "zephyr": None,  # Zephyr uses west build, not cargo --target
}

# Platform → CMake toolchain file name (relative to nano-ros cmake/toolchain/).
# None means native (no toolchain file).
PLATFORM_TOOLCHAINS = {
    "native": None,
    "freertos": "arm-freertos-armcm3.cmake",
    "baremetal": "arm-none-eabi.cmake",
    "nuttx": "arm-nuttx.cmake",
    "threadx": "arm-threadx.cmake",
    "zephyr": None,  # Zephyr uses west build
}

# Platform (build_type token) → NANO_ROS_PLATFORM CMake cache value.
# Falls back to the token itself when unmapped.
PLATFORM_CMAKE = {
    "native": "posix",
    "freertos": "freertos_armcm3",
    "baremetal": "baremetal",
    "nuttx": "nuttx",
    "threadx": "threadx",
    "zephyr": "zephyr",
}

# Canonical RMW backends. The orchestration `nros build` path is the
# single source of truth for RMW (it reads `system.target.rmw` from the
# plan; see orchestration/generate.rs). The standalone per-package colcon
# task has no system config in scope, so it takes the RMW from the
# `NANO_ROS_RMW` env var (default `zenoh`) — Phase 172.M, replacing the
# former hardcoded `zenoh`.
RMW_BACKENDS = ("zenoh", "xrce", "cyclonedds")


def resolve_rmw():
    """Resolve the RMW backend for a standalone colcon package build.

    Reads ``NANO_ROS_RMW`` (default ``zenoh``); validates against
    :data:`RMW_BACKENDS`. The orchestration ``nros build`` path does not
    use this — it threads ``system.target.rmw`` from the plan.
    """
    rmw = os.environ.get("NANO_ROS_RMW", "zenoh")
    if rmw not in RMW_BACKENDS:
        raise ValueError(f"NANO_ROS_RMW={rmw!r} not one of {RMW_BACKENDS}")
    return rmw


# SDK environment variables forwarded to CMake and Cargo builds.
SDK_ENV_VARS = (
    "FREERTOS_DIR",
    "LWIP_DIR",
    "FREERTOS_PORT",
    "FREERTOS_CONFIG_DIR",
    "NUTTX_DIR",
    "NUTTX_APPS_DIR",
    "THREADX_DIR",
    "THREADX_CONFIG_DIR",
    "NETX_DIR",
    "NETX_CONFIG_DIR",
)


def parse_nros_type(pkg_type):
    """Parse 'ros.nros.<lang>.<platform>' into (lang, platform).

    >>> parse_nros_type('ros.nros.rust.freertos')
    ('rust', 'freertos')
    >>> parse_nros_type('ros.nros.c.native')
    ('c', 'native')
    """
    parts = pkg_type.split(".")
    if len(parts) != 4 or parts[0] != "ros" or parts[1] != "nros":
        raise ValueError(
            f"Invalid nros build type: '{pkg_type}'. Expected 'ros.nros.<lang>.<platform>'."
        )
    return parts[2], parts[3]


# Lock file to ensure workspace-level binding generation runs only once
_bindings_generated = False


class NrosBuildTask(TaskExtensionPoint):
    """Build task for nano-ros packages (nros.<lang>.<platform>).

    Supports Rust (cargo), C (cmake), and C++ (cmake) packages
    targeting native, FreeRTOS, bare-metal, NuttX, ThreadX, and Zephyr.

    The build type is encoded in package.xml:
        <build_type>nros.rust.freertos</build_type>

    Board-specific configuration is handled by the board crate (Rust)
    or CMake platform module (C/C++), not by this task.
    """

    def __init__(self):  # noqa: D107
        super().__init__()
        satisfies_version(TaskExtensionPoint.EXTENSION_POINT_VERSION, "^1.0")

    async def build(self, *, additional_hooks=None, skip_hook_creation=False):  # noqa: D102
        pkg = self.context.pkg
        args = self.context.args

        lang, platform = parse_nros_type(pkg.type)
        logger.info(f"Building nros package '{pkg.name}' (lang={lang}, platform={platform})")

        # Generate workspace-level bindings (once, first package triggers)
        rc = await self._generate_bindings(pkg, args, lang)
        if rc:
            return rc

        if platform == "zephyr":
            return await self._build_zephyr(pkg, args, lang, additional_hooks, skip_hook_creation)
        elif lang == "rust":
            return await self._build_rust(pkg, args, platform, additional_hooks, skip_hook_creation)
        elif lang in ("c", "cpp"):
            return await self._build_cmake(
                pkg, args, lang, platform, additional_hooks, skip_hook_creation
            )
        else:
            logger.error(f"Unknown language: {lang}")
            return 1

    async def _generate_bindings(self, pkg, args, lang):
        """Generate workspace-level interface bindings (once per workspace).

        Checks NrosBindingAugmentation for collected interface dependencies,
        then runs `nros generate-rust` once for the workspace as needed.
        Output goes to build/nros_bindings/<interface_pkg>/.
        """
        global _bindings_generated
        if _bindings_generated:
            return 0

        # Set flag BEFORE any await to prevent concurrent tasks from
        # also entering this path (Python asyncio is cooperative —
        # the flag must be set before yielding control).
        _bindings_generated = True

        from colcon_nano_ros.nros_augmentation import NrosBindingAugmentation

        interface_deps = getattr(NrosBindingAugmentation, "_interface_deps", set())
        if not interface_deps:
            return 0

        # Derive workspace build dir from args.build_base
        # args.build_base is per-package (e.g., build/hello_nros)
        # We want the workspace-level build/ dir
        build_root = Path(args.build_base).resolve().parent
        bindings_dir = build_root / "nros_bindings"
        bindings_dir.mkdir(parents=True, exist_ok=True)

        NrosBindingAugmentation._bindings_dir = bindings_dir

        logger.info(f"Generating nros bindings for: {', '.join(sorted(interface_deps))}")

        # Generate Rust bindings if any Rust nros packages exist.
        if NrosBindingAugmentation._needs_rust:
            manifest = bindings_dir / "package.xml"
            deps_xml = "\n".join(f"  <depend>{dep}</depend>" for dep in sorted(interface_deps))
            manifest.write_text(
                '<?xml version="1.0"?>\n'
                '<package format="3">\n'
                "  <name>nros_colcon_bindings</name>\n"
                "  <version>0.0.0</version>\n"
                "  <description>Generated nano-ros binding manifest</description>\n"
                '  <maintainer email="noreply@example.com">nano-ros</maintainer>\n'
                "  <license>Apache-2.0</license>\n"
                f"{deps_xml}\n"
                "</package>\n",
                encoding="utf-8",
            )
            cmd = [
                "nros",
                "generate-rust",
                "--manifest",
                str(manifest),
                "--output",
                str(bindings_dir),
            ]
            rc = await run(self.context, cmd)
            if rc and rc.returncode != 0:
                logger.error("Failed to generate Rust bindings with nros")
                return rc.returncode

        # C/C++ bindings are handled by CMake's nano_ros_generate_interfaces()
        # during the cmake build step — not here.

        logger.info(f"nros bindings generated in {bindings_dir}")
        return 0

    async def _build_rust(self, pkg, args, platform, additional_hooks, skip_hook_creation):
        """Build a Rust nros package with cargo."""
        pkg_path = Path(pkg.path)
        install_base = Path(args.install_base)

        # 1. Build with cargo
        cmd = ["cargo", "build", "--release", "--quiet"]

        target = PLATFORM_TARGETS.get(platform)
        if target:
            cmd.extend(["--target", target])

        # Select the RMW from the single source (NANO_ROS_RMW env; Phase
        # 172.M). nano-ros example/component crates expose mutually-exclusive
        # `rmw-{zenoh,xrce,cyclonedds}` features with `default = ["rmw-zenoh"]`,
        # so `--no-default-features --features rmw-<rmw>` selects exactly one.
        cmd.extend(["--no-default-features", "--features", f"rmw-{resolve_rmw()}"])

        # Forward SDK environment variables for embedded platforms.
        # The board crate's build.rs reads these to locate FreeRTOS/lwIP/etc.
        env = dict(os.environ)
        for var in SDK_ENV_VARS:
            val = os.environ.get(var)
            if val:
                env[var] = val

        rc = await run(self.context, cmd, cwd=str(pkg_path), env=env)
        if rc and rc.returncode != 0:
            return rc.returncode

        # 2. Find and install binary targets
        binaries = self._find_rust_binaries(pkg_path, target)
        if not binaries:
            logger.warning(f"No binary targets found for '{pkg.name}'")

        lib_dir = install_base / "lib" / pkg.name
        lib_dir.mkdir(parents=True, exist_ok=True)
        for bin_path in binaries:
            dest = lib_dir / bin_path.name
            shutil.copy2(str(bin_path), str(dest))
            logger.info(f"Installed {bin_path.name} → {dest}")

        # 3. Install package.xml
        share_dir = install_base / "share" / pkg.name
        share_dir.mkdir(parents=True, exist_ok=True)
        pkg_xml = pkg_path / "package.xml"
        if pkg_xml.exists():
            shutil.copy2(str(pkg_xml), str(share_dir / "package.xml"))

        # 4. Create ament resource index marker
        resource_dir = share_dir / "ament_index" / "resource_index" / "packages"
        resource_dir.mkdir(parents=True, exist_ok=True)
        (resource_dir / pkg.name).touch()

        # 5. Create environment hooks
        if not skip_hook_creation:
            hooks = additional_hooks or []
            hooks.extend(
                create_environment_hook(
                    "ament_prefix_path",
                    install_base,
                    pkg.name,
                    "AMENT_PREFIX_PATH",
                    "",
                    mode="prepend",
                )
            )
            default_hooks = create_environment_hooks(str(install_base), pkg.name)
            create_environment_scripts(
                pkg, args, default_hooks=default_hooks, additional_hooks=hooks
            )

        return 0

    async def _build_zephyr(self, pkg, args, lang, additional_hooks, skip_hook_creation):
        """Build a Zephyr nros package with west build."""
        pkg_path = Path(pkg.path).resolve()
        install_base = Path(args.install_base).resolve()
        build_dir = Path(args.build_base).resolve()

        # Zephyr requires ZEPHYR_BASE and west workspace
        zephyr_base = os.environ.get("ZEPHYR_BASE")
        if not zephyr_base:
            logger.error(
                "ZEPHYR_BASE not set. Source the Zephyr environment first:\n"
                "  source <zephyr-workspace>/zephyr/zephyr-env.sh"
            )
            return 1

        # Read board from config.toml or environment
        board = os.environ.get("NROS_ZEPHYR_BOARD", "native_sim")

        # 1. west build
        cmd = [
            "west",
            "build",
            "-b",
            board,
            "-d",
            str(build_dir),
            "-p",
            "auto",
            str(pkg_path),
        ]

        # CMAKE_PREFIX_PATH is for third-party SDK find_package(); nano-ros is
        # add_subdirectory (no find_package(NanoRos) since Phase 140).
        prefix_paths = [str(install_base.parent)]
        env_prefix = os.environ.get("CMAKE_PREFIX_PATH", "")
        if env_prefix:
            prefix_paths.append(env_prefix)
        west_defs = [f"-DCMAKE_PREFIX_PATH={';'.join(prefix_paths)}"]

        # Select the RMW Kconfig overlay (Phase 172.M): base prj.conf + the
        # per-RMW overlay, the same shape the `just zephyr` recipes use. RMW
        # from the single source (NANO_ROS_RMW env).
        west_defs.append(f"-DCONF_FILE=prj.conf;prj-{resolve_rmw()}.conf")

        cmd.extend(["--", *west_defs])

        rc = await run(self.context, cmd)
        if rc and rc.returncode != 0:
            return rc.returncode

        # 2. Find and install binary
        # native_sim produces zephyr.exe; hardware boards produce zephyr.elf
        for bin_name in ("zephyr.exe", "zephyr.elf"):
            bin_path = build_dir / "zephyr" / bin_name
            if bin_path.exists():
                lib_dir = install_base / "lib" / pkg.name
                lib_dir.mkdir(parents=True, exist_ok=True)
                dest = lib_dir / bin_name
                shutil.copy2(str(bin_path), str(dest))
                logger.info(f"Installed {bin_name} → {dest}")
                break
        else:
            logger.warning(f"No Zephyr binary found in {build_dir / 'zephyr'}")

        # 3. Install package.xml
        share_dir = install_base / "share" / pkg.name
        share_dir.mkdir(parents=True, exist_ok=True)
        pkg_xml = pkg_path / "package.xml"
        if pkg_xml.exists():
            shutil.copy2(str(pkg_xml), str(share_dir / "package.xml"))

        # 4. Create ament resource index marker
        resource_dir = share_dir / "ament_index" / "resource_index" / "packages"
        resource_dir.mkdir(parents=True, exist_ok=True)
        (resource_dir / pkg.name).touch()

        # 5. Create environment hooks
        if not skip_hook_creation:
            hooks = additional_hooks or []
            hooks.extend(
                create_environment_hook(
                    "ament_prefix_path",
                    install_base,
                    pkg.name,
                    "AMENT_PREFIX_PATH",
                    "",
                    mode="prepend",
                )
            )
            default_hooks = create_environment_hooks(str(install_base), pkg.name)
            create_environment_scripts(
                pkg, args, default_hooks=default_hooks, additional_hooks=hooks
            )

        return 0

    def _resolve_toolchain(self, toolchain_name, install_base):
        """Find a CMake toolchain file by name.

        Searches:
        1. NROS_TOOLCHAIN_DIR env var
        2. nano-ros install prefix (share/nano-ros/cmake/toolchain/)
        3. Relative to package's cmake/ directory
        """
        # Check env var first
        toolchain_dir = os.environ.get("NROS_TOOLCHAIN_DIR")
        if toolchain_dir:
            path = Path(toolchain_dir) / toolchain_name
            if path.exists():
                return str(path)

        # Check nano-ros install prefix
        for candidate in [
            install_base.parent / "share" / "nano-ros" / "cmake" / "toolchain" / toolchain_name,
            install_base.parent / "cmake" / "toolchain" / toolchain_name,
        ]:
            if candidate.exists():
                return str(candidate)

        return None

    def _find_rust_binaries(self, pkg_path, target):
        """Find built binary targets using cargo metadata."""
        try:
            result = subprocess.run(
                ["cargo", "metadata", "--no-deps", "--format-version=1"],
                capture_output=True,
                text=True,
                cwd=str(pkg_path),
            )
            if result.returncode != 0:
                logger.warning("cargo metadata failed, scanning target/ dir")
                return self._find_binaries_fallback(pkg_path, target)

            metadata = json.loads(result.stdout)
            bin_names = []
            for package in metadata.get("packages", []):
                for t in package.get("targets", []):
                    if "bin" in t.get("kind", []):
                        bin_names.append(t["name"])

            if not bin_names:
                return []

            # Resolve binary paths
            if target:
                target_dir = pkg_path / "target" / target / "release"
            else:
                target_dir = pkg_path / "target" / "release"

            binaries = []
            for name in bin_names:
                bin_path = target_dir / name
                if bin_path.exists():
                    binaries.append(bin_path)
            return binaries

        except Exception as e:
            logger.warning(f"cargo metadata failed: {e}")
            return self._find_binaries_fallback(pkg_path, target)

    async def _build_cmake(self, pkg, args, lang, platform, additional_hooks, skip_hook_creation):
        """Build a C/C++ nros package with CMake."""
        pkg_path = Path(pkg.path).resolve()
        install_base = Path(args.install_base).resolve()
        build_dir = Path(args.build_base).resolve()

        # 1. CMake configure
        cmd = [
            "cmake",
            "-S",
            str(pkg_path),
            "-B",
            str(build_dir),
            f"-DCMAKE_INSTALL_PREFIX={install_base}",
        ]

        # Pass CMAKE_PREFIX_PATH for third-party SDK find_package() (CycloneDDS,
        # NetX Duo, FreeRTOS-Kernel). nano-ros itself is consumed via
        # add_subdirectory — there is no find_package(NanoRos) since Phase 140.
        prefix_paths = [str(install_base.parent)]
        env_prefix = os.environ.get("CMAKE_PREFIX_PATH", "")
        if env_prefix:
            prefix_paths.append(env_prefix)
        cmd.append(f"-DCMAKE_PREFIX_PATH={';'.join(prefix_paths)}")

        # RMW + platform for the NanoRos CMake config. RMW comes from the
        # single source (NANO_ROS_RMW env; Phase 172.M) instead of a hardcoded
        # `zenoh`; platform from the parsed build_type token instead of a
        # hardcoded `freertos_armcm3`.
        cmd.append(f"-DNANO_ROS_RMW={resolve_rmw()}")
        cmd.append(f"-DNANO_ROS_PLATFORM={PLATFORM_CMAKE.get(platform, platform)}")

        # Cross-compilation: pass toolchain file for embedded platforms.
        # The toolchain file is resolved from NROS_TOOLCHAIN_DIR env var
        # or the nano-ros install prefix.
        toolchain_name = PLATFORM_TOOLCHAINS.get(platform)
        if toolchain_name:
            toolchain_file = self._resolve_toolchain(toolchain_name, install_base)
            if toolchain_file:
                cmd.append(f"-DCMAKE_TOOLCHAIN_FILE={toolchain_file}")
            else:
                logger.warning(
                    f"Toolchain file '{toolchain_name}' not found — cross-compilation may fail"
                )

        # Forward SDK environment variables as CMake -D flags.
        # The CMake platform support module reads these.
        for var in SDK_ENV_VARS:
            val = os.environ.get(var)
            if val:
                cmd.append(f"-D{var}={val}")

        rc = await run(self.context, cmd)
        if rc and rc.returncode != 0:
            return rc.returncode

        # 2. CMake build
        rc = await run(self.context, ["cmake", "--build", str(build_dir)])
        if rc and rc.returncode != 0:
            return rc.returncode

        # 3. CMake install (uses CMAKE_INSTALL_PREFIX set during configure)
        rc = await run(self.context, ["cmake", "--install", str(build_dir)])
        if rc and rc.returncode != 0:
            return rc.returncode

        # 4. Install package.xml if not already installed by CMake
        share_dir = install_base / "share" / pkg.name
        share_dir.mkdir(parents=True, exist_ok=True)
        pkg_xml = pkg_path / "package.xml"
        dest_xml = share_dir / "package.xml"
        if pkg_xml.exists() and not dest_xml.exists():
            shutil.copy2(str(pkg_xml), str(dest_xml))

        # 5. Create ament resource index marker
        resource_dir = share_dir / "ament_index" / "resource_index" / "packages"
        resource_dir.mkdir(parents=True, exist_ok=True)
        (resource_dir / pkg.name).touch()

        # 6. Create environment hooks
        if not skip_hook_creation:
            hooks = additional_hooks or []
            hooks.extend(
                create_environment_hook(
                    "ament_prefix_path",
                    install_base,
                    pkg.name,
                    "AMENT_PREFIX_PATH",
                    "",
                    mode="prepend",
                )
            )
            default_hooks = create_environment_hooks(str(install_base), pkg.name)
            create_environment_scripts(
                pkg, args, default_hooks=default_hooks, additional_hooks=hooks
            )

        return 0

    def _find_binaries_fallback(self, pkg_path, target):
        """Fallback: scan target/release/ for executable files."""
        if target:
            target_dir = pkg_path / "target" / target / "release"
        else:
            target_dir = pkg_path / "target" / "release"

        if not target_dir.exists():
            return []
        return [
            f
            for f in target_dir.iterdir()
            if f.is_file() and os.access(str(f), os.X_OK) and not f.suffix  # skip .d, .rlib, etc.
        ]
