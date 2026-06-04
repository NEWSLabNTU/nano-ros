# Licensed under the Apache License, Version 2.0

from pathlib import Path

from colcon_core.logging import colcon_logger
from colcon_core.package_augmentation import PackageAugmentationExtensionPoint
from colcon_core.plugin_system import satisfies_version

logger = colcon_logger.getChild(__name__)


class RustBindingAugmentation(PackageAugmentationExtensionPoint):
    """Generate workspace-level ROS 2 Rust bindings during package augmentation phase.

    This extension runs AFTER package discovery but BEFORE any build tasks start.
    It receives ALL discovered packages and generates bindings once for the entire workspace.

    This is the architecturally correct way to handle workspace-level operations in colcon,
    avoiding fragile directory scanning and respecting colcon's package selection flags.
    """

    PRIORITY = 90  # Run after most other augmentations

    def __init__(self):
        """Initialize the RustBindingAugmentation extension."""
        super().__init__()
        satisfies_version(PackageAugmentationExtensionPoint.EXTENSION_POINT_VERSION, "^1.0")
        self._bindings_generated = False

    def augment_packages(self, descs, *, additional_argument_names=None):
        """Collect all Cargo packages for dependency-aware binding generation.

        Args:
            descs: Collection of ALL package descriptors discovered by colcon
            additional_argument_names: Additional argument names (unused)
        """
        # Only collect packages once for the entire workspace
        if self._bindings_generated:
            return

        # Collect ALL Cargo packages (both application and interface packages)
        # We need to discover their ROS dependencies to know which bindings to generate
        cargo_descriptors = {}
        for desc in descs:
            pkg_path = Path(desc.path)

            # Check if package has a Cargo.toml file
            if (pkg_path / "Cargo.toml").exists():
                # Store the FULL descriptor (includes parsed dependencies from package.xml)
                cargo_descriptors[desc.name] = desc
                logger.debug(f"Found Cargo package: {desc.name} at {pkg_path}")

        if not cargo_descriptors:
            logger.debug("No Cargo packages found in workspace")
            return

        logger.info(f"Discovered {len(cargo_descriptors)} Cargo packages via colcon")

        # Store Cargo package descriptors for dependency discovery during build phase
        # Each descriptor includes dependencies parsed from package.xml by Colcon
        # We'll use these to discover which ROS interface packages need bindings
        RustBindingAugmentation._cargo_descriptors = cargo_descriptors

        # Also store all descriptors for potential recursive dependency resolution
        RustBindingAugmentation._all_descriptors = set(descs)

        self._bindings_generated = True

        # Note: We don't call super().augment_packages() because we're doing
        # workspace-level operations, not per-package augmentation


# Class variables to share discovered packages with build tasks
RustBindingAugmentation._cargo_descriptors = {}
RustBindingAugmentation._all_descriptors = set()
