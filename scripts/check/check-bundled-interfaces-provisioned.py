#!/usr/bin/env python3
"""Every interface package the CLI bundles is one the images install (issue 2278).

Since issue 2278 a cmake configure refuses an interface that the ament install
lacks, instead of silently taking the CLI's bundled copy
(`packages/cli/interfaces/<pkg>`). So the ROS install of every image we run
gates on must carry every bundled package, or a fixture that uses one refuses
at configure. On 2026-11-07 tier 2 on the self-hosted runner refused
`examples/templates/rclcpp-compat-smoke` for `diagnostic_msgs`: the image's
list (`[prereq.ros-interface-packages] ros_package`) named 4 of the 10.

The rule: the bundled package directories are a subset of that list. Both
are read, neither is authored here. Buildless.
"""

from __future__ import annotations

import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
BUNDLED = ROOT / "packages" / "cli" / "interfaces"
INDEX = ROOT / "nros-sdk-index.toml"
ROW = "ros-interface-packages"


def missing(bundled: set[str], listed: set[str]) -> list[str]:
    return sorted(bundled - listed)


def bundled_packages() -> set[str]:
    return {d.name for d in BUNDLED.iterdir() if (d / "package.xml").is_file()}


def main() -> int:
    assert missing({"a", "b"}, {"a"}) == ["b"]
    assert missing({"a"}, {"a", "c"}) == []
    bundled = bundled_packages()
    if not bundled:
        print(f"check-bundled-interfaces-provisioned: no package under {BUNDLED.relative_to(ROOT)}; "
              "refusing to report OK over nothing", file=sys.stderr)
        return 1
    row = tomllib.loads(INDEX.read_text()).get("prereq", {}).get(ROW, {})
    listed = set(row.get("ros_package", []))
    gap = missing(bundled, listed)
    if gap:
        print(f"check-bundled-interfaces-provisioned: the CLI bundles {', '.join(gap)}, which "
              f"[prereq.{ROW}] ros_package does not install (issue 2278).\n"
              "  A configure that uses one refuses on a host whose ROS lacks it. Add it to the row.",
              file=sys.stderr)
        return 1
    print(f"check-bundled-interfaces-provisioned OK — {len(bundled)} bundled package(s), "
          f"all in [prereq.{ROW}]")
    return 0


if __name__ == "__main__":
    sys.exit(main())
