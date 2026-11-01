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

A feature that only builds for an embedded target (`platform-freertos`,
`platform-zephyr`, ...) is linted by the CROSS half: `cargo clippy --lib
--no-default-features --target <triple>` with the environment the board that
ships it states in its `nros-board.toml` (port, config dir, C compiler). The
board descriptor is read, never restated here, so a run builds the crate the
way that board's images do. A board whose `[unstable]` asks for build-std
(NuttX: no prebuilt std) runs on the pinned nightly with those `-Z` flags, and
a platform whose C build reads a configured OS tree (NuttX's `config.h`)
FAILS, naming the setup step, when that tree is missing.

Scheduled, never merge-gating: it is ~330 clippy runs, each a partial rebuild
of the crate's dependency graph under a new feature set.

Four lists keep it honest:

  SKIP      a feature this sweep does not lint, each with the reason and where
            it IS covered. A crate's feature that is in neither the run nor this
            list is a script error, so a NEW feature joins the sweep by default.
  EXPECTED  a combination that is refused on purpose (a `compile_error!`).
            It must still FAIL, and with the named text, so an entry cannot
            outlive the refusal it records.
  CROSS     the cross runs. A host SKIP that says "cross" must name one here
            and every run here must be such a SKIP — both directions, so a
            `_CROSS` reason cannot claim coverage that does not exist.
  STANDALONE  the crates no `-p` can name (issues 1309, 1960): every
            root-workspace `exclude` entry under `packages/` that is its own
            cargo root, whether or not it has its own `[workspace]` or a
            tracked lock. Each is linted from its own manifest, for the board
            or triple its row names (or the target the leaf states), then
            `rustfmt --check`ed. The set is DERIVED from the root manifest, and
            every member of it is a run or a STANDALONE_SKIP with a reason,
            both directions.

Run: python3 scripts/check-feature-sweep.py [--crate NAME] [--half host|cross] [--list]
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
    "NROS_C_API_SHIPPED_FEATURES",
    "alloc,env,log,panic-platform,rmw-cffi,platform-posix,ros-humble",
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

# A platform that only builds for an embedded target: linted by the CROSS half
# below, which must hold a run for every SKIP carrying this exact reason.
_CROSS = "an embedded target; linted by this sweep's CROSS half"
# ESP32 is dormant (issue 1525): its board descriptor states no C compiler, so
# the C shims nros-c compiles fall back to a riscv64 gcc that has no rv32
# libraries. The pure-Rust `nros-platform` arm is still cross-swept.
_ESP32_DORMANT = (
    "ESP32 is dormant (issue 1525) and its board states no C compiler for the "
    "C shims; nros-platform's esp32 arm is cross-swept"
)
# The feature set nros-c / nros-cpp ship in on an embedded target: what
# `nros_feature_set()` (cmake/NanoRosFeatureSet.cmake) assembles for RMW none,
# plus the default panic handler a staticlib needs to build at all.
C_EMBEDDED = "ros-humble,rmw-cffi,alloc,panic-platform"
# A staticlib whose platform leaves the panic handler and allocator to the
# image that links it (Zephyr's Rust runtime, a bare-metal board crate), so the
# archive alone cannot link. Its code is nros-rmw-zenoh's, which IS cross-swept.
_IMAGE_COMPLETES = (
    "the linking image supplies the panic handler and allocator; the code is "
    "nros-rmw-zenoh's, whose CROSS run lints it"
)
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
        # phase-359 W10: nros-node has no `std` feature; cargo refuses one it
        # does not declare, which failed all 58 of this crate's rows.
        "base": "alloc",
        "axes": ["", "rmw-cffi"],
        "expected": {
            ("signal-fd-wake", ""): "add \"rmw-cffi\" to this crate's features",
        },
        "skip": {},
    },
    "nros": {
        # phase-359 W10 stage B: `nros` has no `std` feature any more, so a
        # base naming it failed every row of this crate.
        "base": "alloc",
        "axes": ["", "rmw-cffi"],
        "expected": {
            # Issue 1765 — the size probe's own build only; build.rs refuses it.
            ("ffi-size-markers", ""): "is enabled outside the size probe",
            ("ffi-size-markers", "rmw-cffi"): "is enabled outside the size probe",
            # phase-359 W10 stage B — the TOML loader needs the bridge's own
            # `std`, which the consumer names at its dep-site; `nros` has no
            # `std` to forward, so the bridge's compile_error! refuses it here.
            ("config", ""): "parses TOML via serde",
            ("config", "rmw-cffi"): "parses TOML via serde",
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
        # phase-359 W10 stage C — `panic-halt` beside the base's `panic-platform`
        # used to be refused (the base was `std`, whose handler was a second
        # one, E0152). The base has no libstd now, and `panic-platform`'s
        # handler stands down for `panic-halt`, so the pair composes.
        "expected": {},
        "skip": {
            "platform-esp32-qemu": _ESP32_DORMANT,
            "platform-freertos": _CROSS,
            "platform-mps2-an385": _CROSS,
            "platform-nuttx": _CROSS,
            "platform-stm32f4": _CROSS,
            "platform-threadx": _CROSS,
            "platform-zephyr": _CROSS,
        },
    },
    "nros-cpp": {
        "base": C_SHIPPED,
        "no_default": True,
        "axes": [""],
        # phase-359 W10 stage C — `panic-halt` beside the base's `panic-platform`
        # used to be refused (the base was `std`, whose handler was a second
        # one, E0152). The base has no libstd now, and `panic-platform`'s
        # handler stands down for `panic-halt`, so the pair composes.
        "expected": {},
        "skip": {
            "platform-esp32-qemu": _ESP32_DORMANT,
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
            "platform-nuttx": _CROSS,
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
            **{
                p: _CROSS
                for p in (
                    "platform-esp32-qemu",
                    "platform-freertos",
                    "platform-mps2-an385",
                    "platform-stm32f4",
                    "platform-threadx",
                    "platform-zephyr",
                )
            },
            "platform-nuttx": _CROSS,
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
    # The two staticlibs are what an RTOS image links; on the host they build
    # as `std` (+ POSIX for zenoh), which is the shape their host tests use.
    "nros-rmw-zenoh-staticlib": {
        "base": "std,platform-posix",
        "axes": [""],
        "expected": {},
        "skip": {
            **{
                p: _CROSS
                for p in (
                    "platform-freertos",
                    "platform-threadx",
                    "platform-threadx-std",
                    "platform-zephyr-baremetal",
                )
            },
            "platform-bare-metal": _IMAGE_COMPLETES,
            "platform-zephyr": _IMAGE_COMPLETES,
            "platform-nuttx": _CROSS,
        },
    },
    "nros-rmw-xrce-cffi-staticlib": {
        "base": "std",
        "axes": [""],
        "expected": {
            # A bare-metal panic handler; `std` (and the test harness) bring
            # their own, so the pair is refused by the compiler.
            ("panic-halt", ""): "duplicate lang item",
        },
        "skip": {},
    },
    # -- testing support -----------------------------------------------------
    # nros-tests' features gate its test TARGETS (`required-features`).
    # Compiling one needs no fixture; only running it does.
    "nros-tests": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "zpico-alloc": {"base": "", "axes": [""], "expected": {}, "skip": {}},
    "nros-condvar-contract": {"base": "", "axes": [""], "expected": {}, "skip": {}},
}

# The cross runs: (crate, feature it covers, extra features, board or triple).
# A BOARD names `packages/boards/<board>/nros-board.toml`, whose `cargo_config`
# supplies the triple and the environment (FreeRTOS port, ThreadX config dir,
# the C compiler) — read, never restated. A bare TRIPLE is for a platform whose
# build needs no board environment. Every run is `--no-default-features`.
CROSS = [
    ("nros-platform", "platform-freertos", "alloc", "nros-board-mps2-an385-freertos"),
    ("nros-platform", "platform-mps2-an385", "alloc", "thumbv7m-none-eabi"),
    ("nros-platform", "platform-stm32f4", "alloc", "thumbv7em-none-eabihf"),
    ("nros-platform", "platform-esp32-qemu", "alloc", "riscv32imc-unknown-none-elf"),
    ("nros-platform", "platform-threadx", "alloc", "nros-board-threadx-qemu-riscv64"),
    ("nros-platform", "platform-zephyr", "alloc", "thumbv7m-none-eabi"),
    ("nros-rmw-zenoh", "platform-freertos", "alloc", "nros-board-mps2-an385-freertos"),
    ("nros-rmw-zenoh", "platform-threadx", "alloc", "nros-board-threadx-qemu-riscv64"),
    ("nros-rmw-zenoh", "platform-zephyr", "alloc", "thumbv7m-none-eabi"),
    ("nros-rmw-zenoh", "platform-bare-metal", "alloc", "thumbv7m-none-eabi"),
    ("zpico-sys", "freertos", "link-ip", "nros-board-mps2-an385-freertos"),
    ("zpico-sys", "threadx", "link-ip", "nros-board-threadx-qemu-riscv64"),
    ("zpico-sys", "zephyr", "link-ip", "thumbv7m-none-eabi"),
    ("zpico-sys", "bare-metal", "link-ip", "thumbv7m-none-eabi"),
    *(
        (crate, plat, C_EMBEDDED, where)
        for crate in ("nros-c", "nros-cpp")
        for plat, where in (
            ("platform-freertos", "nros-board-mps2-an385-freertos"),
            ("platform-threadx", "nros-board-threadx-qemu-riscv64"),
            ("platform-zephyr", "thumbv7m-none-eabi"),
            # issue 2206 -- the CAS-less ARM triple a Cortex-M0+ Zephyr image
            # (Pico W) builds; nros-cpp's take path had a core `fetch_add`.
            ("platform-zephyr", "thumbv6m-none-eabi"),
            ("platform-mps2-an385", "thumbv7m-none-eabi"),
            ("platform-stm32f4", "thumbv7em-none-eabihf"),
        )
    ),
    # NuttX: the board's own build-std (pinned nightly) and a configured tree.
    # These lint with the NIGHTLY clippy, whose lint set runs ahead of the
    # pinned stable's — the price of a target with no prebuilt std.
    ("nros-platform", "platform-nuttx", "alloc", "nros-board-nuttx-qemu"),
    ("zpico-sys", "nuttx", "link-ip", "nros-board-nuttx-qemu"),
    ("nros-rmw-zenoh", "platform-nuttx", "alloc", "nros-board-nuttx-qemu"),
    ("nros-rmw-zenoh-staticlib", "platform-nuttx", "", "nros-board-nuttx-qemu"),
    ("nros-c", "platform-nuttx", C_EMBEDDED, "nros-board-nuttx-qemu"),
    ("nros-cpp", "platform-nuttx", C_EMBEDDED, "nros-board-nuttx-qemu"),
    ("nros-rmw-zenoh-staticlib", "platform-freertos", "", "nros-board-mps2-an385-freertos"),
    ("nros-rmw-zenoh-staticlib", "platform-threadx", "", "nros-board-threadx-qemu-riscv64"),
    ("nros-rmw-zenoh-staticlib", "platform-threadx-std", "std", "nros-board-threadx-linux"),
    ("nros-rmw-zenoh-staticlib", "platform-zephyr-baremetal", "", "thumbv7m-none-eabi"),
]


# Issue 1309 — a root-workspace `exclude` entry takes a crate out of every `-p`
# run above, and out of `--workspace` too, so an excluded crate that is its own
# cargo root is linted by nothing unless it is named here. Clippy lints only
# the members of the root it runs in, so being BUILT as some other root's path
# dependency (a fixture, an example leaf) is not being linted (issue 1960).
#
# Each run names WHERE its target comes from, the CROSS half's way: a BOARD
# (`packages/boards/<board>/nros-board.toml` supplies the triple, the
# environment and any build-std, read through `cross_shape`), a bare TRIPLE,
# `LEAF` (the root states its own target: a tracked `.cargo/config.toml`
# `[build] target`, or a `system.toml` image `nros sync` renders), or `HOST`.
# `check_standalone_where` refuses a LEAF row whose root states no target, and
# a row that overrides a target the root does state.
#
# The rest is the root's SHAPE (`standalone_shape`), never restated here:
#   - a tracked `Cargo.lock` is linted `--locked` against that lock;
#     otherwise the lock is seeded from the pin set, resolved, and held to the
#     pins (`nros ws seed-lock` / `nros ws check-lock`);
#   - a root whose manifest path-depends on a `generated/` or `build/` tree is
#     rendered by `nros sync` first, as its fixture build does;
#   - an explicit `[workspace] members` list is linted `--workspace`; an
#     implicit one is the root package alone, because its other members are
#     the `generated/` message crates sync wrote, which are not ours to judge;
#   - the lint is the root's default targets (library and binaries) in its OWN
#     target dir (one workspace root per dir, issue 0616), then `rustfmt
#     --check` on the same packages with the pinned nightly.
HOST = "host"
LEAF = "leaf"
STANDALONE = {
    # issue 1309's set: no lock of their own, no [workspace]
    "packages/platform/nros-platform-mps2-an385": "thumbv7m-none-eabi",
    "packages/platform/nros-platform-stm32f4": "thumbv7em-none-eabihf",
    "packages/platform/nros-platform-esp32-qemu": "riscv32imc-unknown-none-elf",
    "packages/testing/qemu-smoltcp-bridge": "nros-board-mps2-an385",
    # issue 1960 — own [workspace], fixture binaries
    "packages/testing/nros-bench/executor-fairness": HOST,
    "packages/testing/nros-bench/stress-zenoh": HOST,
    "packages/testing/nros-bench/stress-xrce": HOST,
    "packages/testing/nros-bench/large-msg-xrce": HOST,
    "packages/testing/nros-bench/port-metric": HOST,
    "packages/testing/nros-bench/wcet-cycles-qemu": LEAF,
    "packages/testing/nros-bench/large-msg-baremetal": LEAF,
    "packages/testing/nros-tests/bins/cdr-roundtrip-qemu": LEAF,
    "packages/testing/nros-tests/bins/lan9118-qemu": LEAF,
    # own [workspace] (+ tracked lock)
    "packages/testing/nros-smoke/stm32f4-smoltcp-echo": LEAF,
    "packages/testing/nros-smoke/esp32-hello-world": LEAF,
    "packages/boards/nros-board-nuttx-qemu": "nros-board-nuttx-qemu",
    "packages/boards/nros-board-zephyr": "thumbv7m-none-eabi",
    # tracked lock (a `locked_roots` entry)
    "packages/drivers/net/nros-smoltcp": "nros-board-mps2-an385",
    "packages/drivers/net/openeth-smoltcp": "riscv32imc-unknown-none-elf",
    "packages/drivers/serial/cmsdk-uart": "nros-board-mps2-an385",
    "packages/drivers/serial/stm32f4-usart": "thumbv7em-none-eabihf",
    "packages/rmw/zenoh/zpico-serial": "nros-board-mps2-an385",
    "packages/boards/mps2-an385-pac": "nros-board-mps2-an385",
    "packages/boards/nros-board-mps2-an385": "nros-board-mps2-an385",
    "packages/boards/nros-board-mps2-an385-freertos": "nros-board-mps2-an385-freertos",
    "packages/boards/nros-board-s32z270-freertos": "nros-board-s32z270-freertos",
    "packages/boards/nros-board-mps3-an536-freertos": "nros-board-mps3-an536-freertos",
    "packages/boards/nros-board-threadx-linux": "nros-board-threadx-linux",
    "packages/boards/nros-board-threadx-qemu-riscv64": "nros-board-threadx-qemu-riscv64",
    "packages/reference/stm32f4-porting/polling": LEAF,
    "packages/reference/stm32f4-porting/rtic": LEAF,
}
# A STANDALONE row linted `--no-default-features`, with the reason its
# defaults cannot build on the cargo road. Every key is a STANDALONE row.
# Empty since issue 1966: the two Cortex-R52 boards were here because zpico-sys's
# size probe compiled without the platform's `[arch.cortex-r52]` profile.
STANDALONE_NO_DEFAULT = {}
STANDALONE_SKIP = {
    "packages/verification/nros-verification": (
        "a Verus crate: `verus!` erases its spec and proof items unless Verus "
        "builds it (`cfg(verus_keep_ghost)`), so plain rustc cannot resolve the "
        "spec functions its modules import; it is checked by `just verify-verus` "
        "with the SDK-store Verus this sweep does not provision"
    ),
    "packages/boards/nros-board-esp32-qemu": (
        "ESP32 is dormant (issue 1525), and this board resolves three crates the "
        "pin set does not name (esp-alloc, allocator-api2, linked_list_allocator); "
        "pinning them for a board nothing builds is a pin-set change with no user"
    ),
}


def _tracked(pattern):
    return set(
        subprocess.run(
            ["git", "ls-files", pattern], cwd=ROOT, capture_output=True, text=True, check=True
        ).stdout.split()
    )


def standalone_roots():
    """Every root-workspace `exclude` entry under `packages/` that has a
    `[package]`, declares a Rust target, and is its own cargo root: no
    ANCESTOR inside the checkout holds a `[workspace]` (a member of a nested
    workspace is linted, or not, with that workspace). A root's own
    `[workspace]` and a tracked `Cargo.lock` change HOW it is linted
    (`standalone_shape`), never WHETHER — issue 1309 derived only the roots
    with neither, which left issue 1960's 28 built and linted by no lane."""
    toml = _toml()
    exclude = toml.loads((ROOT / "Cargo.toml").read_text(encoding="utf8"))["workspace"]["exclude"]

    def is_workspace(d):
        m = d / "Cargo.toml"
        return m.is_file() and "workspace" in toml.loads(m.read_text(encoding="utf8"))

    roots = set()
    for entry in exclude:
        if not entry.startswith("packages/"):
            continue
        d = ROOT / entry
        manifest = d / "Cargo.toml"
        if not manifest.is_file():
            continue
        doc = toml.loads(manifest.read_text(encoding="utf8"))
        if "package" not in doc:
            continue
        if not ((d / "src").is_dir() or "lib" in doc or "bin" in doc):
            continue
        if any(is_workspace(a) for a in d.parents if a != ROOT and ROOT in a.parents):
            continue
        roots.add(entry)
    return roots


def standalone_shape(rel, tracked_locks=None, tracked_configs=None):
    """What a root's own files say about how it is built — see STANDALONE."""
    toml = _toml()
    d = ROOT / rel
    doc = toml.loads((d / "Cargo.toml").read_text(encoding="utf8"))
    if tracked_locks is None:
        tracked_locks = _tracked("*Cargo.lock")
    if tracked_configs is None:
        tracked_configs = _tracked("*.cargo/config.toml")
    members = doc.get("workspace", {}).get("members")
    deps = {}
    for key in ("dependencies", "build-dependencies", "dev-dependencies"):
        deps.update(doc.get(key, {}))
    renders = any(
        isinstance(v, dict) and str(v.get("path", "")).startswith(("generated/", "build/"))
        for v in deps.values()
    )
    config = f"{rel}/.cargo/config.toml"
    states_target = False
    if config in tracked_configs:
        states_target = "target" in toml.loads((ROOT / config).read_text(encoding="utf8")).get(
            "build", {}
        )
    system = d / "system.toml"
    image = system.is_file() and "image" in toml.loads(system.read_text(encoding="utf8"))
    return {
        "package": doc["package"]["name"],
        "locked": f"{rel}/Cargo.lock" in tracked_locks,
        "workspace": bool(members),
        "renders": renders or image,
        "states_target": states_target or image,
        "image": image,
    }


def check_standalone_where(rel, where, shape):
    """A LEAF row needs a root that states its target; any other row must not
    override one the root states (its images are built for THAT target)."""
    if where == LEAF and not shape["states_target"]:
        raise SystemExit(
            f"check-feature-sweep: STANDALONE takes {rel}'s target from the leaf, and "
            "it states none (no tracked `.cargo/config.toml` `[build] target`, no "
            "`system.toml` image) — name its board or triple"
        )
    if where != LEAF and shape["states_target"]:
        raise SystemExit(
            f"check-feature-sweep: STANDALONE gives {rel} `{where}`, but the root states "
            "its own target — use LEAF, so the lint builds what its images build"
        )


def check_standalone_coverage(roots, runs, skip, no_default=()):
    """Both directions: a derived root is a run or a reasoned skip, and a run or
    a skip names a root the derivation still finds."""
    for r in sorted(roots - set(runs) - set(skip)):
        raise SystemExit(
            f"check-feature-sweep: {r} is excluded from the root workspace and is "
            "its own cargo root, so no `-p` run lints it (issues 1309, 1960) — add it to "
            "STANDALONE with its target, or to STANDALONE_SKIP with the reason"
        )
    for r in sorted((set(runs) | set(skip)) - roots):
        raise SystemExit(
            f"check-feature-sweep: STANDALONE names {r}, which is no longer an "
            "excluded standalone root — drop the entry"
        )
    for r in sorted(set(runs) & set(skip)):
        raise SystemExit(f"check-feature-sweep: {r} is both a STANDALONE run and a skip")
    for r in sorted(set(no_default) - set(runs)):
        raise SystemExit(
            f"check-feature-sweep: STANDALONE_NO_DEFAULT names {r}, which is no "
            "STANDALONE run — drop the entry"
        )


def check_cross_coverage(crates, cross):
    """Both directions: a `_CROSS` skip has a run, and a run is a `_CROSS` skip."""
    runs = {(c, f) for c, f, _, _ in cross}
    claimed = {
        (c, f) for c, cfg in crates.items() for f, why in cfg["skip"].items() if why == _CROSS
    }
    for c, f in sorted(claimed - runs):
        raise SystemExit(
            f"check-feature-sweep: {c} skips `{f}` as linted by the CROSS half, "
            "and CROSS has no run for it — add one, or give the skip its real reason"
        )
    for c, f in sorted(runs - claimed):
        raise SystemExit(
            f"check-feature-sweep: CROSS lints {c} `{f}`, which {c}'s host SKIP "
            "does not mark `_CROSS` — the two lists disagree"
        )


def _toml():
    try:
        import tomllib
    except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
        import tomli as tomllib
    return tomllib


def board_entry(board):
    """The first `[[board]]` entry of a board descriptor."""
    path = ROOT / "packages" / "boards" / board / "nros-board.toml"
    doc = _toml().loads(path.read_text(encoding="utf8"))
    return doc["board"][0] if isinstance(doc["board"], list) else doc["board"]


def board_cargo_config(board):
    """The `cargo_config` a board descriptor carries, parsed."""
    return _toml().loads(board_entry(board).get("cargo_config", ""))


def pinned_nightly():
    """The repo's pinned nightly (`tools/rust-toolchain.toml`), which every
    `-Z build-std` build here uses — read, never restated."""
    doc = _toml().loads((ROOT / "tools" / "rust-toolchain.toml").read_text(encoding="utf8"))
    return doc["toolchain"]["channel"]


def build_std_args(cargo_config):
    """`+<nightly> -Z build-std=...` for a board whose `[unstable]` asks for
    build-std (a target with no prebuilt std, e.g. NuttX), else nothing."""
    unstable = cargo_config.get("unstable", {})
    if not unstable.get("build-std"):
        return []
    args = [f"+{pinned_nightly()}", "-Z", "build-std=" + ",".join(unstable["build-std"])]
    if unstable.get("build-std-features"):
        args += ["-Z", "build-std-features=" + ",".join(unstable["build-std-features"])]
    return args


def platform_tree(platform):
    """(env, error) for a platform whose C build needs a configured OS tree.

    NuttX: zpico-sys's platform manifest requires `NUTTX_DIR`, and its headers
    include the GENERATED `include/nuttx/config.h`, which only exists after the
    kernel is configured. Located through the one ladder (`source_locate`),
    never joined by hand."""
    if platform != "nuttx":
        return {}, None
    sys.path.insert(0, str(ROOT / "scripts" / "lib"))
    from source_locate import source_dir

    tree = source_dir("nuttx-kernel")
    if tree is None or not (Path(tree) / "include" / "nuttx" / "config.h").is_file():
        return {}, (
            "no configured NuttX tree (nuttx-kernel, include/nuttx/config.h): run "
            "`nros setup qemu-armv7a-nuttx` then `scripts/nuttx/build-nuttx.sh`"
        )
    return {"NUTTX_DIR": str(tree)}, None


def board_env(cargo_config, root):
    """The board's `[env]`, with `${workspace}` resolved to this checkout."""
    out = {}
    for k, v in cargo_config.get("env", {}).items():
        value = v["value"] if isinstance(v, dict) else v
        out[k] = str(value).replace("${workspace}", str(root))
    return out


def cross_shape(where):
    """(triple or None for the host, env, cargo prefix args, unmet precondition)
    for a CROSS run's board-or-triple."""
    if (ROOT / "packages" / "boards" / where / "nros-board.toml").is_file():
        cfg = board_cargo_config(where)
        env = board_env(cfg, ROOT)
        tree_env, unmet = platform_tree(board_entry(where).get("platform"))
        env.update(tree_env)
        return cfg.get("build", {}).get("target"), env, build_std_args(cfg), unmet
    return where, {}, [], None


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


# Issue 1824 — this sweep used to delete the FLAT target-dir sizes headers
# before every run: in a warm dir a cached nros-c unit did not rerun, so the
# shared header was the last run's and nros-cpp's verifier refused it. The flat
# copy is gone, and the verifier reads nros-c's OWN `$OUT_DIR` for the same unit
# graph, so a warm dir is correct as it stands.


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


def cross_clippy(name, feats, triple, board_env_vars, target_dir, prefix=(), unmet=None):
    """A CROSS run: the library only (tests are a host build), no defaults.
    An unmet precondition FAILS the run, naming the remedy — never a skip."""
    if unmet:
        return 1, f"error: {unmet}"
    cmd = ["cargo", *prefix[:1], "clippy", *prefix[1:], "--quiet", "-p", name, "--lib", "--locked"]
    cmd += ["--no-default-features", "--features", feats]
    if triple:
        cmd += ["--target", triple]
    cmd += ["--", "-D", "warnings"]
    env = dict(os.environ, CARGO_TARGET_DIR=str(target_dir), **board_env_vars)
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, env=env)
    return p.returncode, p.stderr


def standalone_shape_of_where(where):
    """(triple or None, env, cargo prefix args, unmet) for a STANDALONE row:
    `HOST` and `LEAF` pass no `--target` (the leaf's own config, read from its
    directory, decides); a board or triple is the CROSS half's `cross_shape`."""
    if where in (HOST, LEAF):
        return None, {}, [], None
    return cross_shape(where)


def image_config_args(d):
    """The `--config` a cargo run inside a rendered `system.toml` leaf needs:
    `scripts/build/leaf-settings.sh`'s `nros_leaf_inside_config_args`, the one
    rule the example lint and the fixture build share. Nothing when sync wired
    the settings into the leaf's own `.cargo/config.toml` as an `include`."""
    cfg = d / ".cargo" / "config.toml"
    if cfg.is_file() and "/nros-cargo.toml" in cfg.read_text(encoding="utf8"):
        return []
    out = []
    for f in sorted(d.glob("build/*/nros-cargo.toml")):
        out += ["--config", str(f.relative_to(d))]
    return out


def standalone_clippy(rel, where, target_dir, cli, shape=None, no_default=False):
    """A STANDALONE run: render, lint, hold the lock, check the format.

    An unlocked root runs unlocked on purpose: a seeded lock holds the registry
    pins only, and the first resolve completes it with the root's path crates,
    as `nros build`'s does. The pins are then checked by `nros ws check-lock`,
    so a crate the pin set does not name FAILS here rather than resolving
    freely. A root with a tracked lock is held to THAT lock (`--locked`), which
    `check-cargo-registry-pins` already holds to the pins."""
    if cli is None:
        return 1, "error: no nros CLI to seed the lock from the pin set: run `just setup-cli`"
    d = ROOT / rel
    shape = shape or standalone_shape(rel)
    triple, board_vars, prefix, unmet = standalone_shape_of_where(where)
    if unmet:
        return 1, f"error: {unmet}"
    if shape["renders"]:
        sync = subprocess.run([cli, "sync"], cwd=d, capture_output=True, text=True)
        if sync.returncode != 0:
            return sync.returncode, "error: `nros sync` failed\n" + sync.stdout + sync.stderr
    if not shape["locked"]:
        seed = subprocess.run([cli, "ws", "seed-lock", str(d)], capture_output=True, text=True)
        if seed.returncode != 0:
            return seed.returncode, seed.stderr
    scope = ["--workspace"] if shape["workspace"] else []
    cmd = ["cargo", *prefix[:1], "clippy", *prefix[1:], "--quiet"]
    cmd += ["--manifest-path", str(d / "Cargo.toml"), *scope]
    if shape["locked"]:
        cmd.append("--locked")
    if no_default:
        cmd.append("--no-default-features")
    if triple:
        cmd += ["--target", triple]
    if shape["image"]:
        cmd += image_config_args(d)
    cmd += ["--", "-D", "warnings"]
    env = dict(
        os.environ,
        CARGO_TARGET_DIR=str(Path(target_dir) / "standalone" / rel.replace("/", "-")),
        **board_vars,
    )
    p = subprocess.run(cmd, cwd=d, capture_output=True, text=True, env=env)
    if p.returncode != 0:
        return p.returncode, p.stderr
    if not shape["locked"]:
        lock = subprocess.run([cli, "ws", "check-lock", str(d)], capture_output=True, text=True)
        if lock.returncode != 0:
            return lock.returncode, lock.stdout + lock.stderr
    # `-p <package>` (or `--all` over explicit members), never a bare
    # `fmt --check`: a rendered root's implicit members include the generated
    # message crates, which rustfmt has no business judging.
    fmt = ["cargo", f"+{pinned_nightly()}", "fmt", "--check", "--manifest-path"]
    fmt += [str(d / "Cargo.toml")]
    fmt += ["--all"] if shape["workspace"] else ["-p", shape["package"]]
    f = subprocess.run(fmt, cwd=d, capture_output=True, text=True)
    if f.returncode != 0:
        return f.returncode, "error: rustfmt --check found diffs\n" + f.stdout + f.stderr
    return 0, ""


def verdict(expect, rc, err):
    """(passed, reason). An EXPECTED refusal passes only if it still refuses,
    with its named text; anything else passes only if clippy is clean."""
    if expect:
        if rc != 0 and expect in err:
            return True, ""
        why = "built clean" if rc == 0 else "failed with other text"
        return False, f"expected refusal `{expect}`, {why}"
    return rc == 0, ""


# Issue 1987 — how much of one build script's `--- stderr` a FAIL prints. A C
# compile error is a few lines; a panic with a backtrace is not, and the job
# log is the only place this output lands.
BUILD_SCRIPT_STDERR_LINES = 60


def diagnostic_lines(err):
    """The lines of a failed run's stderr that say WHY it failed.

    clippy's own findings are `error…`, `  -->` and `Diff in` lines. A build
    script's failure is not: cargo prints one `error: failed to run custom
    build command` line and puts the reason under `Caused by:`, in the
    script's `--- stderr` section and on `cargo:warning=` lines. Keeping only
    the first kind printed the crate's name and nothing else, every night
    (issue 1987)."""
    out, in_stderr, kept, dropped = [], False, 0, 0
    for line in err.splitlines():
        body = line.strip()
        # The section ends where cargo's indentation does.
        if in_stderr and line and not line.startswith(" "):
            in_stderr = False
            if dropped:
                out.append(f"  ... ({dropped} more build-script stderr line(s))")
        if in_stderr:
            if not body:
                continue
            if kept < BUILD_SCRIPT_STDERR_LINES:
                out.append(line)
                kept += 1
            else:
                dropped += 1
            continue
        if line.startswith(("error", "  -->", "Diff in")):
            out.append(line)
        elif body.startswith(("process didn't exit successfully", "cargo:warning=")):
            out.append(line)
        elif body == "--- stderr":
            out.append(line)
            in_stderr, kept, dropped = True, 0, 0
    if in_stderr and dropped:
        out.append(f"  ... ({dropped} more build-script stderr line(s))")
    return out


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
    # A `_CROSS` skip needs a CROSS run, and a CROSS run needs that skip.
    crates = {"x": {"base": "", "axes": [""], "expected": {}, "skip": {"p": _CROSS}}}
    check_cross_coverage(crates, [("x", "p", "", "t")])
    for bad_crates, bad_cross in ((crates, []), ({"x": {**crates["x"], "skip": {}}}, [("x", "p", "", "t")])):
        try:
            check_cross_coverage(bad_crates, bad_cross)
        except SystemExit:
            pass
        else:
            raise AssertionError("a one-sided cross claim was accepted")
    # A derived standalone root needs a run or a skip, and an entry needs a root.
    check_standalone_coverage({"a", "b"}, {"a": "t"}, {"b": "r"})
    for roots, runs, skip in (
        ({"a", "b"}, {"a": "t"}, {}),
        ({"a"}, {"a": "t", "gone": "t"}, {}),
        ({"a"}, {"a": "t"}, {"a": "r"}),
    ):
        try:
            check_standalone_coverage(roots, runs, skip)
        except SystemExit:
            pass
        else:
            raise AssertionError("a one-sided standalone claim was accepted")
    # A no-default-features reason must name a STANDALONE run.
    check_standalone_coverage({"a"}, {"a": "t"}, {}, {"a": "why"})
    for roots, runs, skip, nd in (({"a"}, {"a": "t"}, {}, {"gone": "why"}),):
        try:
            check_standalone_coverage(roots, runs, skip, nd)
        except SystemExit:
            pass
        else:
            raise AssertionError("a one-sided standalone claim was accepted")
    # No CLI is a FAILED run naming the remedy, never a skip.
    assert standalone_clippy("x", "t", "/nonexistent", None)[0] == 1
    # A LEAF row needs a root that states its target; a board or triple row
    # must not override one the root states.
    check_standalone_where("a", LEAF, {"states_target": True})
    check_standalone_where("a", "t", {"states_target": False})
    for where, states in ((LEAF, False), ("t", True), (HOST, True)):
        try:
            check_standalone_where("a", where, {"states_target": states})
        except SystemExit:
            pass
        else:
            raise AssertionError("a STANDALONE row disagreeing with its root's target was accepted")
    # HOST and LEAF pass no --target and no board environment.
    assert standalone_shape_of_where(HOST) == (None, {}, [], None)
    assert standalone_shape_of_where(LEAF) == (None, {}, [], None)
    # A board asking for build-std gets the pinned nightly and the -Z flags;
    # one that does not gets plain cargo.
    args = build_std_args({"unstable": {"build-std": ["core", "alloc"]}})
    assert args[0].startswith("+") and "build-std=core,alloc" in args, args
    assert build_std_args({}) == []
    # An unmet precondition FAILS the run with its remedy; it is never a skip.
    assert cross_clippy("x", "f", None, {}, "/nonexistent", (), "do the thing") == (
        1,
        "error: do the thing",
    )
    # A board's env resolves `${workspace}` and unwraps `{ value = ... }`.
    env = board_env({"env": {"A": "x", "B": {"value": "${workspace}/c", "force": True}}}, "/r")
    assert env == {"A": "x", "B": "/r/c"}, env
    # A build script's failure keeps its REASON, not only the crate's name
    # (issue 1987). The shape is cargo's own, captured from a failing script.
    failed = (
        "warning: b@0.0.0: probe said no\n"
        "error: failed to run custom build command for `b v0.0.0 (/x)`\n"
        "\n"
        "Caused by:\n"
        "  process didn't exit successfully: `/x/build-script-build` (exit status: 1)\n"
        "  --- stdout\n"
        "  cargo:rerun-if-changed=build.rs\n"
        "  cargo:warning=probe said no\n"
        "\n"
        "  --- stderr\n"
        "  x.c:3:10: fatal error: tx_port.h: No such file or directory\n"
        "warning: build failed, waiting for other jobs to finish...\n"
    )
    kept = diagnostic_lines(failed)
    assert any("tx_port.h" in line for line in kept), kept
    assert any("cargo:warning=probe said no" in line for line in kept), kept
    assert not any("rerun-if-changed" in line for line in kept), kept
    assert not any("waiting for other jobs" in line for line in kept), kept
    # ...and a long one is bounded, saying how much it dropped.
    long = "  --- stderr\n" + "".join(f"  line {i}\n" for i in range(100))
    kept = diagnostic_lines(long)
    assert len(kept) == 1 + BUILD_SCRIPT_STDERR_LINES + 1, len(kept)
    assert kept[-1].endswith(f"({100 - BUILD_SCRIPT_STDERR_LINES} more build-script stderr line(s))")
    # A clippy finding still prints as before.
    assert diagnostic_lines("error: unused import\n  --> src/lib.rs:1:5\nnote: x\n") == [
        "error: unused import",
        "  --> src/lib.rs:1:5",
    ]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--crate", action="append", help="sweep only this crate (repeatable)")
    ap.add_argument("--list", action="store_true", help="print the plan, run nothing")
    ap.add_argument("--half", choices=("host", "cross"), help="run one half only")
    ap.add_argument("--self-test", action="store_true", help="run the controls only")
    args = ap.parse_args()
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment.
    self_test()
    check_cross_coverage(CRATES, CROSS)
    check_standalone_coverage(
        standalone_roots(), STANDALONE, STANDALONE_SKIP, STANDALONE_NO_DEFAULT
    )
    locks, configs = _tracked("*Cargo.lock"), _tracked("*.cargo/config.toml")
    for rel, where in STANDALONE.items():
        check_standalone_where(rel, where, standalone_shape(rel, locks, configs))
    if args.self_test:
        print("check-feature-sweep --self-test: OK")
        return 0

    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).with_name(
        Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).name + "-feature-sweep"
    )
    names = args.crate or list(CRATES)
    standalone_names = {Path(rel).name for rel in STANDALONE}
    for name in names:
        if name not in CRATES and name not in standalone_names:
            raise SystemExit(f"check-feature-sweep: {name} is not in CRATES or STANDALONE")
    # (label, expected refusal, thunk) for every run this invocation owns.
    jobs = []
    if args.half != "cross":
        for name in (n for n in names if n in CRATES):
            cfg = CRATES[name]
            for axis, f, feats in plan(name, cfg, crate_features(name)):
                jobs.append(
                    (
                        f"{name} [{feats or '<none>'}]",
                        cfg["expected"].get((f, axis)),
                        lambda n=name, c=cfg, s=feats: clippy(n, c, s, target_dir),
                    )
                )
    if args.half != "host":
        for name, feature, extra, where in CROSS:
            if name not in names:
                continue
            feats = ",".join(x for x in (extra, feature) if x)
            triple, env, prefix, unmet = cross_shape(where)
            label = f"{name} [{feats}] --target {triple or '<host>'}"
            if where != triple:
                label += f" (board {where})"
            jobs.append(
                (
                    label,
                    None,
                    lambda n=name, s=feats, tr=triple, e=env, px=prefix, u=unmet: cross_clippy(
                        n, s, tr, e, target_dir, px, u
                    ),
                )
            )
    if args.half != "host":
        sys.path.insert(0, str(ROOT / "scripts" / "lib"))
        from source_locate import nros_cli_bin

        cli = None if args.list else nros_cli_bin()
        locks, configs = _tracked("*Cargo.lock"), _tracked("*.cargo/config.toml")
        for rel, where in sorted(STANDALONE.items()):
            name = Path(rel).name
            if args.crate and name not in args.crate:
                continue
            shape = standalone_shape(rel, locks, configs)
            nd = rel in STANDALONE_NO_DEFAULT
            how = [
                w
                for w, on in (
                    ("locked", shape["locked"]),
                    ("synced", shape["renders"]),
                    ("--no-default-features", nd),
                )
                if on
            ]
            jobs.append(
                (
                    f"{name} --manifest-path {rel} ({where}{', ' + ', '.join(how) if how else ''})",
                    None,
                    lambda r=rel, w=where, sh=shape, n=nd: standalone_clippy(
                        r, w, target_dir, cli, sh, n
                    ),
                )
            )
    failures = []
    total = 0
    for label, expect, run in jobs:
        total += 1
        if args.list:
            print(("EXPECT-FAIL " if expect else "") + label)
            continue
        rc, err = run()
        ok, why = verdict(expect, rc, err)
        if ok:
            print(f"  ok  {label}{' refused as expected' if expect else ''}")
        else:
            print(f"  FAIL {label}{': ' + why if why else ''}")
            if not expect:
                for line in diagnostic_lines(err):
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
        print(
            "  standalone: [nros sync] [nros ws seed-lock <dir>] && cargo clippy --manifest-path "
            "<dir>/Cargo.toml [--locked] [--target <triple>] -- -D warnings "
            "[&& nros ws check-lock <dir>] && cargo +<nightly> fmt --check -p <package>"
        )
        return 1
    print(f"check-feature-sweep: OK — {total} combination(s) across {len(names)} crate(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
