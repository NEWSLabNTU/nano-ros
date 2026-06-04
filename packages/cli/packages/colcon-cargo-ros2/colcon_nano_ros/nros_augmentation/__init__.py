# Licensed under the Apache License, Version 2.0

import os
from pathlib import Path

from colcon_core.logging import colcon_logger
from colcon_core.package_augmentation import PackageAugmentationExtensionPoint
from colcon_core.plugin_system import satisfies_version

logger = colcon_logger.getChild(__name__)


def _is_interface_package(pkg_name):
    """Check if a ROS package is an interface package (has .msg/.srv/.action).

    Checks AMENT_PREFIX_PATH for installed interface files.
    """
    ament_prefix = os.environ.get("AMENT_PREFIX_PATH", "")
    for prefix in ament_prefix.split(os.pathsep):
        if not prefix:
            continue
        share_dir = Path(prefix) / "share" / pkg_name
        if not share_dir.is_dir():
            continue
        # Check for .msg, .srv, or .action files
        for ext in ("msg", "srv", "action"):
            subdir = share_dir / ext
            if subdir.is_dir() and any(subdir.glob(f"*.{ext}")):
                return True
    return False


class NrosBindingAugmentation(PackageAugmentationExtensionPoint):
    """Collect interface dependencies from nros packages for workspace-level codegen.

    Runs AFTER package discovery but BEFORE build tasks. Collects all
    <depend> entries from nros packages that are interface packages,
    then generates bindings once into build/nros_bindings/.
    """

    PRIORITY = 95  # Run after ROS package augmentation

    # Class-level state shared with build tasks
    _bindings_dir = None  # Path to build/nros_bindings/
    _needs_rust = False  # At least one Rust nros package exists
    _needs_c = False  # At least one C nros package exists
    _needs_cpp = False  # At least one C++ nros package exists
    _generated = False  # Bindings already generated this run

    def __init__(self):  # noqa: D107
        super().__init__()
        satisfies_version(PackageAugmentationExtensionPoint.EXTENSION_POINT_VERSION, "^1.0")

    def augment_packages(self, descs, *, additional_argument_names=None):  # noqa: D102
        if NrosBindingAugmentation._generated:
            return

        # Collect all nros packages and their interface dependencies
        interface_deps = set()
        for desc in descs:
            if not hasattr(desc, "type") or desc.type is None:
                continue
            if not desc.type.startswith("ros.nros."):
                continue

            # Parse language from type
            parts = desc.type.split(".")
            if len(parts) != 4:
                continue
            lang = parts[2]

            if lang == "rust":
                NrosBindingAugmentation._needs_rust = True
            elif lang == "c":
                NrosBindingAugmentation._needs_c = True
            elif lang == "cpp":
                NrosBindingAugmentation._needs_cpp = True

            # Collect interface dependencies from package.xml <depend>
            for dep_name, dep in (desc.dependencies or {}).items():
                for d in dep:
                    if _is_interface_package(d.name):
                        interface_deps.add(d.name)
                        logger.debug(f"Package '{desc.name}' depends on interface '{d.name}'")

        if not interface_deps:
            logger.debug("No interface dependencies found in nros packages")
            NrosBindingAugmentation._generated = True
            return

        logger.info(
            f"nros workspace: {len(interface_deps)} interface packages to "
            f"generate (rust={NrosBindingAugmentation._needs_rust}, "
            f"c={NrosBindingAugmentation._needs_c}, "
            f"cpp={NrosBindingAugmentation._needs_cpp})"
        )

        # Store for build tasks to use
        NrosBindingAugmentation._interface_deps = interface_deps
        NrosBindingAugmentation._generated = True
