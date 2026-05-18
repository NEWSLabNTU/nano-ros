#!/usr/bin/env python3
"""Phase 157.C.9 — generate nros/app_config.h from config.toml.

CLI mirror of the cmake `nano_ros_generate_config_header()` function
from `packages/core/nros-c/cmake/NanoRosReadConfig.cmake`. The
NuttX make-build path can't run cmake to generate this per-example
header — this script reproduces the parse + template substitution
in pure Python so the same `config.toml` files drive both build
paths.

Usage:
  gen-app-config.py <config.toml> <out-path>
      e.g. gen-app-config.py config.toml include/nros/app_config.h

Templates: reads `cmake/templates/nros_app_config.h.in` from the
nano-ros repo, performs `@VAR@` substitution, writes the output.
Defaults match the cmake function — any missing keys / sections in
the user's config.toml fall back to the same constants the cmake
side hard-codes.
"""

from __future__ import annotations

import sys
from pathlib import Path

try:
    import tomllib  # Python 3.11+
except ImportError:
    import tomli as tomllib  # type: ignore[no-redef]

# Defaults mirror cmake `nano_ros_read_config()` (lines 42-62 of
# `packages/core/nros-c/cmake/NanoRosReadConfig.cmake`).
DEFAULTS: dict[str, str] = {
    "NROS_CONFIG_IP": "192,0,3,10",
    "NROS_CONFIG_MAC": "0x02,0x00,0x00,0x00,0x00,0x00",
    "NROS_CONFIG_GATEWAY": "192,0,3,1",
    "NROS_CONFIG_NETMASK": "255,255,255,0",
    "NROS_CONFIG_PREFIX": "24",
    "NROS_CONFIG_ZENOH_LOCATOR": "tcp/127.0.0.1:7447",
    "NROS_CONFIG_DOMAIN_ID": "0",
    "NROS_CONFIG_APP_PRIORITY": "12",
    "NROS_CONFIG_APP_STACK_BYTES": "262144",
    "NROS_CONFIG_ZENOH_READ_PRIORITY": "16",
    "NROS_CONFIG_ZENOH_READ_STACK_BYTES": "5120",
    "NROS_CONFIG_ZENOH_LEASE_PRIORITY": "16",
    "NROS_CONFIG_ZENOH_LEASE_STACK_BYTES": "5120",
    "NROS_CONFIG_POLL_PRIORITY": "16",
    "NROS_CONFIG_POLL_INTERVAL_MS": "5",
}

PREFIX_TO_NETMASK: dict[int, str] = {
    8: "255,0,0,0",
    16: "255,255,0,0",
    24: "255,255,255,0",
    32: "255,255,255,255",
}


def ip_to_c(ip: str) -> str:
    return ip.replace(".", ",")


def mac_to_c(mac: str) -> str:
    return ",".join("0x" + part for part in mac.split(":"))


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print("usage: gen-app-config.py <config.toml> <out-path>", file=sys.stderr)
        return 2
    src = Path(argv[1])
    out = Path(argv[2])
    repo_root = Path(__file__).resolve().parents[2]
    template = repo_root / "cmake" / "templates" / "nros_app_config.h.in"
    if not template.exists():
        print(f"error: template not found: {template}", file=sys.stderr)
        return 1

    vars_ = dict(DEFAULTS)
    vars_["NROS_CONFIG_SOURCE"] = str(src)

    if src.exists():
        with src.open("rb") as f:
            cfg = tomllib.load(f)
    else:
        cfg = {}

    network = cfg.get("network", {})
    if "ip" in network:
        vars_["NROS_CONFIG_IP"] = ip_to_c(network["ip"])
    if "mac" in network:
        vars_["NROS_CONFIG_MAC"] = mac_to_c(network["mac"])
    if "gateway" in network:
        vars_["NROS_CONFIG_GATEWAY"] = ip_to_c(network["gateway"])
    if "netmask" in network:
        vars_["NROS_CONFIG_NETMASK"] = ip_to_c(network["netmask"])
    if "prefix" in network:
        prefix = int(network["prefix"])
        vars_["NROS_CONFIG_PREFIX"] = str(prefix)
        if prefix in PREFIX_TO_NETMASK:
            vars_["NROS_CONFIG_NETMASK"] = PREFIX_TO_NETMASK[prefix]

    zenoh = cfg.get("zenoh", {})
    if "locator" in zenoh:
        vars_["NROS_CONFIG_ZENOH_LOCATOR"] = zenoh["locator"]
    if "domain_id" in zenoh:
        vars_["NROS_CONFIG_DOMAIN_ID"] = str(zenoh["domain_id"])

    sched = cfg.get("scheduling", {})
    sched_map = {
        "app_priority": "NROS_CONFIG_APP_PRIORITY",
        "app_stack_bytes": "NROS_CONFIG_APP_STACK_BYTES",
        "zenoh_read_priority": "NROS_CONFIG_ZENOH_READ_PRIORITY",
        "zenoh_read_stack_bytes": "NROS_CONFIG_ZENOH_READ_STACK_BYTES",
        "zenoh_lease_priority": "NROS_CONFIG_ZENOH_LEASE_PRIORITY",
        "zenoh_lease_stack_bytes": "NROS_CONFIG_ZENOH_LEASE_STACK_BYTES",
        "poll_priority": "NROS_CONFIG_POLL_PRIORITY",
        "poll_interval_ms": "NROS_CONFIG_POLL_INTERVAL_MS",
    }
    for key, var in sched_map.items():
        if key in sched:
            vars_[var] = str(sched[key])

    content = template.read_text()
    for var, val in vars_.items():
        content = content.replace(f"@{var}@", val)

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(content)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
