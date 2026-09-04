#!/usr/bin/env python3
"""A qemu recipe every user SOURCE-BUILDS must pin its optional backends.

WHY THIS EXISTS

qemu's configure auto-detects: a feature whose `--enable-`/`--disable-` is not
passed stays `auto`, and `dependency(..., required: get_option(<f>))` then
resolves against whatever dev headers the BUILD HOST carries. The binary links
what it found. For a recipe that ships a `dist` this is survivable — the release
job bundles the closure and the pin is the tarball — but `[tool.esp32-qemu]` has
NO dist, so every user runs the recipe on their own machine and the runtime
dependency set becomes a property of that machine.

Issue 1006 measured the consequence: `check-dist-runtime-deps` against a source
build of `[tool.esp32-qemu]` found 69 sonames no `[prereq.*]` declares — audio,
X11, curl/ssh/usb/dbus — none of which a headless `-M esp32c3 -nographic`
emulator ever uses. Declaring them was the WRONG fix (they are one host's
accident); pinning the configure is the right one.

WHY IT IS NOT `check-dist-runtime-deps`

That gate re-measures a PROVISIONED store, so it can only speak on a host that
has already built the tool — and CI never provisions esp32-qemu (nightly-only).
That is why the gap sat unnoticed. This gate reads the index instead, so it
answers on every host, before any build, and it is what stops the flags being
dropped again.

WHAT IT CHECKS

For every `[tool.<name>]` whose `source.git` names a qemu repository and whose
`source.configure` invokes `./configure`, AND which publishes no `dist.*`:
every feature in HOST_LIB_FEATURES must appear in that configure line as either
`--disable-<f>` or `--enable-<f>`. Enable counts: the rule is that the RECIPE
decides, not the host. A recipe WITH a dist is exempt and reported as such.

KNOWN NARROWING, stated rather than papered over: the dist exemption is about
who RUNS the configure, and it is not airtight. `nros setup` falls back to the
source build on a host key the tool publishes no dist for, so a dist-carrying
recipe can still be source-built by a user on, say, linux-riscv64 — with the
same host-dependent result. The exemption is kept because the alternative is
demanding flags on `[tool.qemu.source]`, whose second copy
(`ci/nano-ros-sdk/scripts/build-qemu.sh`, kept identical by
`check-qemu-configure.sh`) builds the PUBLISHED dist and cannot be changed
without a release build to verify it. Tighten this when someone can rebuild it.

HOST_LIB_FEATURES is the subset of qemu's feature options that reach for a
system library. It is hand-maintained (qemu's option set moves between
releases); adding a feature here is a ratchet, and a new one that a source-only
recipe leaves unpinned is the defect this gate names.

Usage:  check-qemu-source-features.py [nros-sdk-index.toml]
"""

import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# Grouped as the recipe's own comment groups them, so the two read together.
HOST_LIB_FEATURES = [
    # UI — nothing but `-nographic` is ever passed.
    "gtk", "vnc", "sdl", "sdl-image", "curses", "spice", "spice-protocol",
    "dbus-display", "opengl", "virglrenderer", "png", "vte", "xkbcommon",
    # Audio. Per-driver, NOT `--audio-drv-list=`: audio/meson.build builds a
    # module for every driver whose dependency `.found()`, so the list option
    # does not touch the link surface.
    "alsa", "pa", "jack", "oss", "sndio", "pipewire",
    # Host-device passthrough.
    "libusb", "usb-redir", "smartcard", "u2f", "canokey", "brlapi", "libudev",
    "mpath",
    # Block drivers beyond the raw file the emulator boots.
    "curl", "libssh", "rbd", "glusterfs", "libiscsi", "libnfs", "blkio", "fuse",
    # TLS (gcrypt is the crypto backend and is enabled deliberately).
    "gnutls",
    # Remaining host libraries.
    "seccomp", "cap-ng", "attr", "virtfs", "numa", "selinux", "libdw",
    "libcbor", "libkeyutils", "auth-pam", "vde", "af-xdp", "bpf", "linux-aio",
    "linux-io-uring", "libpmem", "libdaxctl", "zstd", "lzo", "snappy", "bzip2",
    "lzfse",
]


def load(path):
    try:
        import tomllib as toml
    except ModuleNotFoundError:
        import tomli as toml
    with open(path, "rb") as fh:
        return toml.load(fh)


def unpinned(configure, features=HOST_LIB_FEATURES):
    """Features the configure line leaves to auto-detection."""
    tokens = set(configure.split())
    return [
        f
        for f in features
        if f"--disable-{f}" not in tokens and f"--enable-{f}" not in tokens
    ]


def audit(index):
    """[(tool, [unpinned features])] for every source-only qemu recipe."""
    problems, seen = [], []
    for name, tool in sorted(index.get("tool", {}).items()):
        source = tool.get("source") or {}
        git = source.get("git", "")
        configure = source.get("configure", "")
        if "qemu" not in git or "./configure" not in configure:
            continue
        if tool.get("dist"):
            # A dist means the release job's build is what users get; its
            # closure is pinned by the tarball, not by this line.
            seen.append((name, "has dist — exempt"))
            continue
        missing = unpinned(configure)
        seen.append((name, "source-only" + (f" — {len(missing)} unpinned" if missing else " — pinned")))
        if missing:
            problems.append((name, missing))
    return problems, seen


def self_test():
    """Prove the check can fail — a negative control nobody runs is a comment."""
    pinned = "./configure --disable-" + " --disable-".join(HOST_LIB_FEATURES)
    checks = [
        ("a fully pinned line is clean", unpinned(pinned) == []),
        ("a bare line is entirely unpinned", unpinned("./configure") == HOST_LIB_FEATURES),
        ("--enable- counts as pinned", unpinned("./configure --enable-curl", ["curl"]) == []),
        (
            "a source-only qemu recipe with a bare configure is a problem",
            audit(
                {
                    "tool": {
                        "t": {"source": {"git": "https://x/qemu", "configure": "./configure"}}
                    }
                }
            )[0]
            != [],
        ),
        (
            "the same recipe WITH a dist is exempt",
            audit(
                {
                    "tool": {
                        "t": {
                            "dist": {"linux-x86_64": {}},
                            "source": {"git": "https://x/qemu", "configure": "./configure"},
                        }
                    }
                }
            )[0]
            == [],
        ),
        (
            "a non-qemu source recipe is out of scope",
            audit({"tool": {"t": {"source": {"git": "https://x/dtc", "configure": "./configure"}}}})[0]
            == [],
        ),
    ]
    bad = [name for name, ok in checks if not ok]
    if bad:
        for b in bad:
            print(f"check-qemu-source-features self-test: FAIL {b}", file=sys.stderr)
        raise SystemExit(1)


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "nros-sdk-index.toml")
    problems, seen = audit(load(path))
    if problems:
        print(
            "check-qemu-source-features: a qemu recipe every user source-builds "
            "leaves backends to\nauto-detection, so its runtime dependency set is "
            "the build HOST's, not the recipe's:\n",
            file=sys.stderr,
        )
        for tool, missing in problems:
            print(f"  [tool.{tool}.source] unpinned: {' '.join(missing)}", file=sys.stderr)
        print(
            "\n  Pass `--disable-<f>` (or `--enable-<f>` if the recipe really wants it)\n"
            "  for each. Issue 1006: an unpinned build linked 69 sonames no [prereq.*]\n"
            "  declares, and declaring THOSE records one host's accident as a contract.",
            file=sys.stderr,
        )
        return 1
    detail = "; ".join(f"{n}: {why}" for n, why in seen) or "no qemu source recipes"
    print(f"check-qemu-source-features OK — {detail}")
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main())
