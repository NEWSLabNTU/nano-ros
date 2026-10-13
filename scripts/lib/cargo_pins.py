"""The Rust registry pin set — the ONE Python spelling (RFC-0104 D2, phase-485 W1).

`nros-sdk-index.toml` names a tracked, registry-only `Cargo.lock`
(`[rust] registry_pins`) holding ONE version per semver-compatible range of
every registry crate any root in the tree resolves. The closed-graph roots it
lists (`[rust] locked_roots`, the repo root and the CLI workspace) keep their
own tracked `Cargo.lock`, and each must be a SUBSET of the pin set — so a
crate cannot sit at one version in the root and another in the examples.

The pin file is in cargo's own lock format, so a leaf lock is seeded with it
directly (RFC-0104 D3, measured in phase-485 M1: cargo keeps seeded versions
its manifests accept and prunes the rest). Only `nros update` WRITES it
(phase-485 W7); this module only reads it, for the repo's gate.
"""

import os
import subprocess

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib

REGISTRY_PREFIX = "registry+"


def compat_range(version):
    """Cargo's semver-compatibility key: `1.4.2` -> `1`, `0.8.3` -> `0.8`,
    `0.0.5` -> `0.0.5`. Two versions with the same key cannot coexist in one
    resolve, so the pin set holds at most one per key."""
    core = version.split("+", 1)[0].split("-", 1)[0]
    major, minor, patch = (int(x) for x in core.split(".")[:3])
    if major:
        return f"{major}"
    if minor:
        return f"0.{minor}"
    return f"0.0.{patch}"


def version_key(version):
    """Order versions numerically on major.minor.patch (pre-release last-wins
    is not needed: no tracked lock holds a pre-release)."""
    core = version.split("+", 1)[0].split("-", 1)[0]
    return tuple(int(x) for x in core.split(".")[:3])


def load_index(root):
    with open(os.path.join(root, "nros-sdk-index.toml"), "rb") as fh:
        return tomllib.load(fh)


def pin_config(root):
    """`(pin file path, [locked root dirs])`, both relative to `root`."""
    rust = load_index(root).get("rust", {})
    pins = rust.get("registry_pins")
    if not pins:
        raise SystemExit("nros-sdk-index.toml: [rust] registry_pins is not set (RFC-0104 D2)")
    return pins, list(rust.get("locked_roots", []))


def load_lock(path):
    with open(path, "rb") as fh:
        return tomllib.load(fh)


def registry_packages(lock):
    """The registry entries of a parsed lock, as a list of package dicts."""
    return [p for p in lock.get("package", []) if p.get("source", "").startswith(REGISTRY_PREFIX)]


def tracked_locks(root):
    """Every tracked `Cargo.lock` in this repository (never a submodule's:
    `git ls-files` does not descend into gitlinks), relative to `root`."""
    out = subprocess.run(
        ["git", "-C", root, "ls-files", "*Cargo.lock"],
        check=True, capture_output=True, text=True,
    ).stdout.split()
    return [p for p in out if not p.startswith("third-party/")]


def lock_path(root_dir):
    return os.path.join(root_dir, "Cargo.lock") if root_dir != "." else "Cargo.lock"


def subset_violations(pins, lock):
    """Registry entries of `lock` the pin set does not hold at that version.

    Returns `[(name, locked version, pinned version or None)]`."""
    pinned = {(p["name"], compat_range(p["version"])): p["version"] for p in registry_packages(pins)}
    bad = []
    for p in registry_packages(lock):
        want = pinned.get((p["name"], compat_range(p["version"])))
        if want != p["version"]:
            bad.append((p["name"], p["version"], want))
    return bad


def one_per_range_violations(pins):
    """`[(name, range, [versions])]` where the pin set holds more than one."""
    seen = {}
    for p in registry_packages(pins):
        seen.setdefault((p["name"], compat_range(p["version"])), []).append(p["version"])
    return [(n, r, vs) for (n, r), vs in sorted(seen.items()) if len(vs) > 1]


# Issue 1873 — a pin range no locked root resolves is either kept ON PURPOSE,
# for a leaf this repo does not lock, or it is what a bump left behind.
# `nros update` never drops a range (`registry_pins::regenerate`), so issue
# 1805's quick-xml 0.37 -> 0.41 bump kept the 0.37.5 pin it moved off for two
# RUSTSEC advisories, and `just check deny` reads no pin file. The keep list
# turns "nobody decided" into a named decision.
LEAF_ONLY_PINS = "locks/leaf-only-pins.toml"


def leaf_only_pins(root):
    """{(name, range): why} from the keep list; {} when the file is absent."""
    path = os.path.join(root, LEAF_ONLY_PINS)
    if not os.path.exists(path):
        return {}
    with open(path, "rb") as fh:
        data = tomllib.load(fh)
    return {(k["name"], k["range"]): k.get("why", "") for k in data.get("keep", [])}


def root_ranges(locks):
    """Every (name, compat range) the given root locks resolve from the registry."""
    return {(p["name"], compat_range(p["version"])) for lock in locks for p in registry_packages(lock)}


def unclaimed_pins(pins, root_locks, keep):
    """Pins neither a locked root resolves nor the keep list names, and keep
    entries that no longer describe an unclaimed pin (stale either way)."""
    used = root_ranges(root_locks)
    pinned = {(p["name"], compat_range(p["version"])): p["version"] for p in registry_packages(pins)}
    orphans = sorted((n, r, v) for (n, r), v in pinned.items() if (n, r) not in used and (n, r) not in keep)
    stale = sorted(k for k in keep if k not in pinned or k in used)
    return orphans, stale
