# Licensed under the Apache License, Version 2.0

"""RFC-0087 D2/D3 / phase-420 W4 — the two facts the entry-point key used to carry.

The 30 `ros.nros.<lang>.<platform>` entry points were the only place the
language and the platform were ever written down, and no `package.xml` ever
declared that build type, so nothing exercised the split. These tests pin where
each fact comes from now, on real manifests written to disk — the failure mode
being guarded against is a reader that quietly returns a plausible default.
"""

from pathlib import Path

import pytest

from colcon_nano_ros import manifest


def write_pkg(tmp_path, name, export_body):
    """Write a minimal package.xml and return its directory.

    The parse is asserted here, not left to the caller. An unparseable manifest
    makes `deploy()` return the host default, which is the expected value in
    two of the tests below — so a broken fixture would make them PASS while
    testing nothing. That is the vacuous-test shape, and this assert is what
    stops it.
    """
    d = tmp_path / name
    d.mkdir()
    body = "\n".join("    " + line for line in export_body.splitlines())
    (d / "package.xml").write_text(
        '<?xml version="1.0"?>\n'
        '<package format="3">\n'
        f"  <name>{name}</name>\n"
        "  <version>0.1.0</version>\n"
        "  <description>fixture</description>\n"
        '  <maintainer email="dev@example.com">Developer</maintainer>\n'
        "  <license>Apache-2.0</license>\n"
        "  <export>\n"
        f"{body}\n"
        "  </export>\n"
        "</package>\n",
        encoding="utf-8",
    )
    from colcon_ros.package_identification.ros import get_package_with_build_type

    pkg, build_type = get_package_with_build_type(str(d))
    assert pkg is not None, f"fixture {name} does not parse as a ROS package"
    assert build_type is not None, f"fixture {name} declares no build type"
    return d


class TestBuildPath:
    """`<build_type>` says which build system runs — nothing else does."""

    def test_the_two_canonical_spellings_resolve(self):
        assert manifest.build_path("ros.nros_cargo") == "cargo"
        assert manifest.build_path("ros.nros_cmake") == "cmake"

    def test_an_ament_type_is_not_claimed(self):
        # `ament_cargo` / `ament_cmake` belong to colcon's own tasks and to
        # interface packages. Claiming them here is the false claim RFC-0087 D2
        # exists to end, pointed the other way.
        for foreign in ("ros.ament_cargo", "ros.ament_cmake", "ros.cmake"):
            assert not manifest.is_nros_type(foreign)
            with pytest.raises(ValueError):
                manifest.build_path(foreign)

    def test_the_retired_key_shape_is_gone(self):
        # The 30 keys W4 deleted. A reader that still accepted them would keep
        # the dead path alive in the one place it could still be reached.
        assert not manifest.is_nros_type("ros.nros.rust.freertos")


class TestDeploy:
    """The platform comes from the package's own consumption tag."""

    def test_deploy_attribute_is_the_platform(self, tmp_path):
        d = write_pkg(
            tmp_path,
            "threadx_pkg",
            "<build_type>nros_cmake</build_type>\n"
            '<nano_ros deploy="threadx" board="riscv64-qemu" rmw="zenoh"/>',
        )
        assert manifest.deploy(d) == "threadx"

    def test_absent_deploy_is_the_host(self, tmp_path):
        # The identical rule `_nros_deploy_to_platform` applies in
        # cmake/NanoRosPackageXml.cmake: no cross-compilation requested means
        # the host axis. Not a stand-in for a fact that went missing.
        d = write_pkg(tmp_path, "host_pkg", "<build_type>nros_cargo</build_type>")
        assert manifest.deploy(d) == "native"

    def test_a_commented_out_tag_does_not_declare_a_platform(self, tmp_path):
        # Issue 0516's class: a manifest that DOCUMENTS the tag in a comment
        # must not be read as declaring it.
        d = write_pkg(
            tmp_path,
            "commented_pkg",
            '<build_type>nros_cargo</build_type>\n<!-- <nano_ros deploy="freertos"/> -->',
        )
        assert manifest.deploy(d) == "native"


class TestNeedsRustBindings:
    """Whether Rust bindings are needed is evidence, not a token split."""

    def test_a_cargo_package_always_needs_them(self, tmp_path):
        d = write_pkg(tmp_path, "cargo_pkg", "<build_type>nros_cargo</build_type>")
        assert manifest.needs_rust_bindings("ros.nros_cargo", d)

    def test_a_cmake_package_carrying_a_crate_needs_them(self, tmp_path):
        # The 19 Zephyr/ThreadX Rust leaves W3 measured: `nros_cmake` packages
        # whose crate CMake imports as a staticlib. The old `lang` token would
        # have called these "rust" and run `cargo build` on them instead.
        d = write_pkg(tmp_path, "zephyr_rust_pkg", "<build_type>nros_cmake</build_type>")
        (d / "Cargo.toml").write_text('[package]\nname = "x"\n', encoding="utf-8")
        assert manifest.needs_rust_bindings("ros.nros_cmake", d)

    def test_a_plain_cmake_package_does_not(self, tmp_path):
        d = write_pkg(tmp_path, "c_pkg", "<build_type>nros_cmake</build_type>")
        assert not manifest.needs_rust_bindings("ros.nros_cmake", Path(d))
