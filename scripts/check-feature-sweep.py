#!/usr/bin/env python3
"""Clippy every feature of the host-buildable crates, one at a time.

Every merge-gating lint runs a FEW feature sets: `--workspace` (features
unified across members, which hides a crate that compiles only because a
sibling turned something on), each crate under its own defaults
(`check-test-targets`), and a handful of named combinations. A feature nothing
names is linted nowhere. The first sweep of nros-node alone (PR #2215) found
five such combinations red, one of which did not COMPILE: under `rmw-lending`
without `rmw-cffi`, the test target's mock implemented neither lending trait.

So this asks, for each crate below (the API and core crates, the RMW
backends, the platform layer and the host board), `cargo clippy --lib --tests -D warnings`
over its BASE set plus each other feature alone (`cargo hack --each-feature`'s
shape, without the dependency). A feature in a one-of GROUP (`platform-*`,
`ros-*`, ...) REPLACES the base's member of that group instead of joining it,
because two members of one group are refused by design.

Scheduled, never merge-gating: it is ~150 clippy runs, each a partial rebuild
of the crate's dependency graph under a new feature set.

Two lists keep it honest:

  SKIP      a feature this sweep does not lint, each with the reason and where
            it IS covered. A crate's feature that is in neither the run nor this
            list is a script error, so a NEW feature joins the sweep by default.
  EXPECTED  a combination that is refused on purpose (a `compile_error!`).
            It must still FAIL, and with the named text, so an entry cannot
            outlive the refusal it records.

Run: python3 scripts/check-feature-sweep.py [--crate NAME] [--list]
"""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The host feature set nros-c / nros-cpp SHIP in. The recipe passes the
# justfile's `C_API_SHIPPED_FEATURES` so it keeps one spelling; the fallback
# is only for a direct run.
C_SHIPPED = os.environ.get(
    "NROS_C_API_SHIPPED_FEATURES", "std,rmw-cffi,platform-posix,ros-humble"
)

ONE_OF_PREFIXES = (
    "platform-",
    "ros-",
    "panic-",
    "max-level-",
    "buffer-size-",
    "dynamic-loggers-",
    "early-records-",
    "rosout-records-",
    "sync-",
)

# Platforms that need a cross toolchain or an SDK the host does not have.
# Their compile is covered by the embedded clippy and the fixture builds.
_CROSS = "needs a cross toolchain or SDK; covered by the embedded clippy and fixture builds"
_RMW_C = "builds a C RMW backend from SDK sources; covered by `check c` and the RMW lanes"
# A default feature every run already has. Swept on its own it would also
# match the `platform-` one-of prefix and REPLACE the base's platform, which
# it is not: it compiles zenoh-pico's `z_*` clock alias TU (issue 0270).
_DEFAULT_ON = "a default feature, so every run of this crate already has it"
_ZPICO_PLATFORMS = (
    "platform-bare-metal",
    "platform-freertos",
    "platform-nuttx",
    "platform-threadx",
    "platform-zephyr",
)

CRATES = {
    "nros-node": {
        "base": "std,alloc",
        "axes": ["", "rmw-cffi"],
        "expected": {
            ("signal-fd-wake", ""): "add \"rmw-cffi\" to this crate's features",
        },
        "skip": {},
    },
    "nros": {
        "base": "std,alloc",
        "axes": ["", "rmw-cffi"],
        "expected": {
            # Issue 1765 — the size probe's own build only; build.rs refuses it.
            ("ffi-size-markers", ""): "is enabled outside the size probe",
            ("ffi-size-markers", "rmw-cffi"): "is enabled outside the size probe",
        },
        "skip": {},
    },
    "nros-core": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-serdes": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-params": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-rmw": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-log": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-c": {
        "base": C_SHIPPED,
        "no_default": True,
        "axes": [""],
        "expected": {
            # A bare-metal panic handler; the `std` the host base needs brings
            # its own, so the pair is refused by the compiler.
            ("panic-halt", ""): "duplicate lang item",
        },
        "skip": {
            "platform-esp32-qemu": _CROSS,
            "platform-freertos": _CROSS,
            "platform-mps2-an385": _CROSS,
            "platform-nuttx": _CROSS,
            "platform-stm32f4": _CROSS,
            "platform-threadx": _CROSS,
            "platform-zephyr": _CROSS,
            "rmw-cyclonedds": _RMW_C,
            "rmw-xrce": _RMW_C,
            "rmw-zenoh": _RMW_C,
        },
    },
    "nros-cpp": {
        "base": C_SHIPPED,
        "no_default": True,
        "axes": [""],
        "expected": {
            # A bare-metal panic handler; the `std` the host base needs brings
            # its own, so the pair is refused by the compiler.
            ("panic-halt", ""): "duplicate lang item",
        },
        "skip": {
            "platform-esp32-qemu": _CROSS,
            "platform-freertos": _CROSS,
            "platform-mps2-an385": _CROSS,
            "platform-nuttx": _CROSS,
            "platform-stm32f4": _CROSS,
            "platform-threadx": _CROSS,
            "platform-zephyr": _CROSS,
        },
    },
    # -- RMW backends ---------------------------------------------------------
    "nros-rmw-cffi": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-bridge": {
        # `alloc` is what every consumer reaches it with (nros forwards
        # `nros-bridge?/alloc`); the `std` axis is the hosted shape.
        "base": "alloc",
        "axes": ["", "std"],
        "expected": {
            ("config", ""): "`config` parses TOML via serde: add \"std\"",
        },
        "skip": {},
    },
    "nros-rmw-zenoh": {
        "base": "std,alloc,platform-posix",
        "axes": [""],
        "expected": {},
        "skip": {
            **{p: _CROSS for p in _ZPICO_PLATFORMS},
            "platform-aliases": _DEFAULT_ON,
        },
    },
    "zpico-sys": {
        "base": "posix,std",
        "axes": [""],
        "expected": {},
        "skip": {
            # zpico-sys names its platforms without a prefix; each one is a
            # cross target, so none is swept and no one-of group is needed.
            "bare-metal": _CROSS,
            "freertos": _CROSS,
            "nuttx": _CROSS,
            "threadx": _CROSS,
            "zephyr": _CROSS,
            "platform-aliases": _DEFAULT_ON,
            "system-zenohpico": "links a system-installed zenoh-pico this host does not "
            "provide; the vendored build is what every lane ships",
        },
    },
    "nros-rmw-cyclonedds": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-rmw-cyclonedds-sys": {
        "base": "platform-posix",
        "axes": [""],
        "expected": {},
        "skip": {},
    },
    # -- platform layer ------------------------------------------------------
    "nros-platform": {
        "base": "std,alloc,platform-posix",
        "axes": [""],
        "expected": {},
        "skip": {
            p: _CROSS
            for p in (
                "platform-esp32-qemu",
                "platform-freertos",
                "platform-mps2-an385",
                "platform-nuttx",
                "platform-stm32f4",
                "platform-threadx",
                "platform-zephyr",
            )
        },
    },
    "nros-platform-api": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-platform-cffi": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-baremetal-common": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    # -- boards and drivers --------------------------------------------------
    "nros-board-linux": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-board-common": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "lan9118-smoltcp": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nvidia-ivc": {
        "base": "",
        "axes": ["", "std"],
        "expected": {("unix-mock", ""): "`unix-mock` uses OS sockets: add \"std\""},
        "skip": {"fsp": "needs the NVIDIA SPE FSP tree (`NV_SPE_FSP_DIR`); covered by `check ivc-fsp`"},
    },
    # -- testing support -----------------------------------------------------
    "zpico-alloc": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-condvar-contract": {"base": "", "axes": [""], "expected": {}, "skip": {}},
}


def group_of(feature):
    for p in ONE_OF_PREFIXES:
        if feature.startswith(p):
            return p
    return None


def compose(base, axis, feature):
    feats = [f for f in base.split(",") if f]
    g = group_of(feature)
    if g:
        feats = [f for f in feats if group_of(f) != g]
    for f in (axis, feature):
        if f and f not in feats:
            feats.append(f)
    return ",".join(feats)


def crate_features(name):
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    for p in json.loads(out)["packages"]:
        if p["name"] == name:
            return sorted(f for f in p["features"] if f != "default")
    raise SystemExit(f"check-feature-sweep: no workspace member named {name}")


def plan(name, cfg, features):
    base = {f for f in cfg["base"].split(",") if f}
    axes = cfg["axes"]
    for f in cfg["skip"]:
        if f not in features:
            raise SystemExit(
                f"check-feature-sweep: {name} SKIP names `{f}`, which the crate "
                "no longer has — drop the entry"
            )
    for f, axis in cfg["expected"]:
        if f not in features:
            raise SystemExit(
                f"check-feature-sweep: {name} EXPECTED names `{f}`, which the "
                "crate no longer has — drop the entry"
            )
    runs = []
    for axis in axes:
        runs.append((axis, "", compose(cfg["base"], axis, "")))
        for f in features:
            if f in base or f in axes or f in cfg["skip"]:
                continue
            runs.append((axis, f, compose(cfg["base"], axis, f)))
    return runs


def clippy(name, cfg, feats, target_dir):
    cmd = ["cargo", "clippy", "--quiet", "-p", name, "--lib", "--tests", "--locked"]
    if cfg.get("no_default"):
        cmd.append("--no-default-features")
    if feats:
        cmd += ["--features", feats]
    cmd += ["--", "-D", "warnings"]
    env = dict(os.environ, CARGO_TARGET_DIR=str(target_dir))
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, env=env)
    return p.returncode, p.stderr


def verdict(expect, rc, err):
    """(passed, reason). An EXPECTED refusal passes only if it still refuses,
    with its named text; anything else passes only if clippy is clean."""
    if expect:
        if rc != 0 and expect in err:
            return True, ""
        why = "built clean" if rc == 0 else "failed with other text"
        return False, f"expected refusal `{expect}`, {why}"
    return rc == 0, ""


def self_test():
    """Negative controls, run on every invocation: each rule must fire."""
    # A one-of group member REPLACES the base's member, never joins it.
    c = compose("std,platform-posix,ros-humble", "", "platform-zephyr").split(",")
    assert "platform-zephyr" in c and "platform-posix" not in c, c
    assert compose("std,alloc", "rmw-cffi", "log") == "std,alloc,rmw-cffi,log"
    cfg = {"base": "std", "axes": [""], "expected": {}, "skip": {}}
    # A feature nobody listed joins the sweep by default.
    assert ("", "brand-new", "std,brand-new") in plan("x", cfg, ["std", "brand-new"])
    # A SKIP or EXPECTED entry naming a feature the crate lost is refused.
    for bad in ({**cfg, "skip": {"gone": "r"}}, {**cfg, "expected": {("gone", ""): "t"}}):
        try:
            plan("x", bad, ["std"])
        except SystemExit:
            pass
        else:
            raise AssertionError("a stale list entry was accepted")
    # An expected refusal must still refuse, with its text.
    assert verdict("boom", 1, "error: boom") == (True, "")
    assert not verdict("boom", 0, "")[0], "a refusal that built clean must fail"
    assert not verdict("boom", 1, "error: other")[0], "other text must fail"
    assert verdict(None, 0, "") == (True, "") and not verdict(None, 1, "x")[0]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--crate", action="append", help="sweep only this crate (repeatable)")
    ap.add_argument("--list", action="store_true", help="print the plan, run nothing")
    ap.add_argument("--self-test", action="store_true", help="run the controls only")
    args = ap.parse_args()
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment.
    self_test()
    if args.self_test:
        print("check-feature-sweep --self-test: OK")
        return 0

    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).with_name(
        Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).name + "-feature-sweep"
    )
    names = args.crate or list(CRATES)
    failures = []
    total = 0
    for name in names:
        if name not in CRATES:
            raise SystemExit(f"check-feature-sweep: {name} is not in CRATES")
        cfg = CRATES[name]
        runs = plan(name, cfg, crate_features(name))
        for axis, f, feats in runs:
            total += 1
            expect = cfg["expected"].get((f, axis))
            label = f"{name} [{feats or '<none>'}]"
            if args.list:
                print(("EXPECT-FAIL " if expect else "") + label)
                continue
            rc, err = clippy(name, cfg, feats, target_dir)
            ok, why = verdict(expect, rc, err)
            if ok:
                print(f"  ok  {label}{' refused as expected' if expect else ''}")
            else:
                print(f"  FAIL {label}{': ' + why if why else ''}")
                if not expect:
                    for line in err.splitlines():
                        if line.startswith(("error", "  -->")):
                            print(f"        {line}")
                failures.append(label)
            sys.stdout.flush()
    if args.list:
        print(f"{total} run(s)")
        return 0
    if failures:
        print(f"check-feature-sweep: {len(failures)} of {total} combination(s) FAILED:")
        for f in failures:
            print(f"  {f}")
        print("  reproduce: cargo clippy -p <crate> --lib --tests --features <set> -- -D warnings")
        return 1
    print(f"check-feature-sweep: OK — {total} combination(s) across {len(names)} crate(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
