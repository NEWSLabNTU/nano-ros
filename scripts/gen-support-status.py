#!/usr/bin/env python3
"""Generate book/src/reference/support-status.md.

The support-status page (CI tiers, platform/RTOS pins, RMW versions,
ROS editions, toolchains) answers "what exactly am I getting, at which
version, tested how hard" — the questions a Rust user answers from
rustc's platform-support page. Version strings are PARSED from the
tree's single sources of truth so a pin bump moves this page in the
same commit or fails the gate:

  * `nros-sdk-index.toml`  — every `[tool.*]` / `[source.*]` version
  * `rust-toolchain.toml`  — the Rust channel + pinned cross targets
  * `packages/rmw/zenoh/zpico-sys/zenoh-pico/version.txt` — cross-check
    against the index's zenoh-pico row (they have disagreed before)

Prose (tier meanings, edition semantics, the router story) is static
data here, with citations.

Run:  python3 scripts/gen-support-status.py [--check]
Gate: just check support-status
"""

import os
import pathlib
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Where the page lives is the page map's answer (RFC-0123 D7), not this script's.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))
import book_pages  # noqa: E402

OUT = str(book_pages.page_file("reference/support-status.md"))
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")


def parse_index_versions():
    """{section-name: version} for every [tool.X] / [source.X]."""
    versions, current = {}, None
    for line in open(INDEX, encoding="utf8"):
        m = re.match(r"\[(tool|source)\.([A-Za-z0-9_-]+)\]", line.strip())
        if m:
            current = m.group(2)
            continue
        if line.strip().startswith("["):
            current = None
            continue
        m = re.match(r'version\s*=\s*"([^"]+)"', line.strip())
        if m and current and current not in versions:
            versions[current] = m.group(1)
    return versions


def rust_toolchain():
    # PARSED, not regex-sliced: the old `text.split("targets")[1]` cut at the
    # first "targets" in a COMMENT once the file grew prose, and the page
    # printed an empty target list.
    try:
        import tomllib
    except ImportError:  # Python < 3.11
        import tomli as tomllib
    with open(os.path.join(ROOT, "rust-toolchain.toml"), "rb") as fh:
        tc = tomllib.load(fh)["toolchain"]
    return tc["channel"], tc.get("targets", [])


def macos_floor(tool):
    """The highest `floor.macos` over `[tool.<tool>]`'s dist rows, or None.

    READ from the index, never spelled here: the floor is MEASURED from the
    artifact (`scripts/sdk/measure-dist-floor.py`) and moves with the pin, so a
    copy on this page would be the second spelling issue 2389 removed from the
    shell scripts (SDK 0.16.9 raised it from 11.0 to 14.0).
    """
    try:
        import tomllib
    except ImportError:  # Python < 3.11
        import tomli as tomllib
    with open(INDEX, "rb") as fh:
        entry = tomllib.load(fh).get("tool", {}).get(tool, {})
    floors = [d.get("floor", {}).get("macos")
              for d in entry.get("dist", {}).values() if isinstance(d, dict)]
    floors = [f for f in floors if f]
    if not floors:
        return None
    return max(floors, key=lambda f: tuple(int(x) for x in f.split(".")))


def zpico_version_txt():
    sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
    from source_locate import located_or_dest  # RFC-0103 D5: the tree is located

    p = os.path.join(located_or_dest("zenoh-pico"), "version.txt")
    return open(p, encoding="utf8").read().strip() if os.path.exists(p) else None


def zpico_version_agrees(index_version, tree_version):
    """The index's `[source.zenoh-pico] version` is the tree's `version.txt`.

    The row is AUTHORED (`nros setup --source zenoh-pico` runs when the
    checkout is absent, so it cannot derive) and this page renders it, so it is
    a claim about the pinned tree and is checked, not trusted. Compared EXACTLY
    on the base version (an `-nrosN` label suffix is allowed): the old 3-char
    prefix test would have passed 1.8.0 against 1.8.1. The index said 1.7.2
    while the pin was 1.8.0 + fork commits, and nothing noticed because this
    gate sat in no lane. Absent tree (a pristine `check fast` clone) is
    NOT VERIFIED, never a quiet pass.
    """
    if tree_version is None:
        sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
        import check_skip

        check_skip.unverified(
            "support-status",
            "zenoh-pico not checked out, so the index's zenoh-pico version is "
            "unread against version.txt — `just setup-worktree`",
        )
        return
    if index_version.split("-")[0] != tree_version.strip():
        sys.exit(f"gen-support-status: zenoh-pico version disagrees — "
                 f"nros-sdk-index.toml [source.zenoh-pico] says {index_version}, "
                 f"the pinned tree's version.txt says {tree_version}. Fix the "
                 "index row (the pin is the truth), then regenerate.")


def self_test():
    zpico_version_agrees("1.8.0", "1.8.0")
    zpico_version_agrees("1.8.0-nros1", "1.8.0\n")
    for idx, tree in (("1.7.2", "1.8.0"), ("1.8.0", "1.8.1"), ("1.8", "1.8.0")):
        try:
            zpico_version_agrees(idx, tree)
        except SystemExit:
            continue
        sys.exit(f"gen-support-status self-test FAILED: {idx} vs {tree} passed")


def v(versions, key):
    if key not in versions:
        sys.exit(f"gen-support-status: `[{key}]` missing from nros-sdk-index.toml "
                 "(renamed? update the curated rows here in the same commit).")
    return versions[key]


def render():
    ver = parse_index_versions()
    channel, targets = rust_toolchain()
    zp_idx = v(ver, "zenoh-pico")
    zsdk_macos = macos_floor("zephyr-sdk")
    if not zsdk_macos:
        sys.exit("gen-support-status: `[tool.zephyr-sdk]` states no macOS dist "
                 "floor, and the Zephyr SDK row quotes it (issue 2389).")
    zpico_version_agrees(zp_idx, zpico_version_txt())

    lines = [
        "<!-- GENERATED by scripts/gen-support-status.py — do not edit by hand.",
        "     Regenerate: python3 scripts/gen-support-status.py",
        "     Gated by:   just check support-status -->",
        "",
        "# Support Status",
        "",
        "What you get, at which version, tested how hard — the analogue of",
        "Rust's platform-support page. Version strings on this page are",
        "parsed from `nros-sdk-index.toml` and `rust-toolchain.toml` (the",
        "pinning SSoTs), so they cannot drift from what `nros setup`",
        "actually installs.",
        "",
        # issue 2267 -- the page is generated from the tree the book is
        # published from, which is `main`, not the release a reader installed.
        "## This page and your release",
        "",
        "This page is generated from the tree the book is published from,",
        "`main`. A release pins its own versions, so a release older than",
        "`main` may provision older ones than the tables below show. For the",
        "release you installed, run `nros doctor --report` in your bringup",
        "package: it prints, dated, the `nros` version, each image's board and",
        "that board's support status, and every tool and source the image pins.",
        "That output is the record to attach to a review.",
        "",
        # issue 2268 -- maintainer ruling 2026-11-07: latest release only.
        "## Release support",
        "",
        "- **Only the newest release is supported.** nano-ros is pre-1.0, and",
        "  each release supersedes the one before it.",
        "- **Fixes reach you in the next release.** A fix lands on `main` and",
        "  ships with the release after it; nothing is backported to an older",
        "  release. To take a fix, move your pin to the release that has it.",
        "- **Release assets stay published.** A release's tag is never deleted",
        "  or moved, and its assets stay on GitHub Releases, so a pinned version",
        "  installs the same bytes later.",
        # issue 2329 -- maintainer ruling 2026-11-07: minimal policy + SBOM.
        "- **No long-term support before 1.0.** A release reaches end of life",
        "  when the next one is published; there is no LTS line to stay on.",
        "- **Security.** Report a vulnerability privately at",
        "  <https://github.com/NEWSLabNTU/nano-ros/security/advisories/new>, not",
        "  in a public issue. A fixed one is published as a GitHub Security",
        "  Advisory naming the affected releases and the release with the fix.",
        "- **SBOM.** Each release publishes `nros-linux-x86_64.cdx.json` beside",
        "  its asset and installs it as `share/nros/sbom.cdx.json`: a CycloneDX",
        "  list of every crate compiled into the toolchain. What one firmware",
        "  image is built from is in your project's `nros-sdk.lock`.",
        "",
        "## CI tiers — what \"supported\" means",
        "",
        "| Tier | Lane | Runs | Guarantee |",
        "|---|---|---|---|",
        "| **1** | `ci` (every change) | Host-native builds + native fixtures, all gates | The logic and the seams are sound. Says nothing about embedded targets. |",
        "| **2** | `ci-matrix` (when core/codegen/cmake move) | 1-wise cover: every platform, language, RMW, and kind appears at least once (~28 % of the coordinate space) | Each axis value builds and runs somewhere — but pairings are NOT covered. |",
        "| **2-nightly** | `ci-matrix-nightly` | Pairwise cover (~70 %) | Platform×language and RMW×language interactions surface here, within a day. |",
        "| **3** | `ci-full` (pre-release) | The whole matrix | Everything the tree claims, verified together. |",
        "",
        "Per-board support levels are generated on their own page:",
        "[Board Support Levels](board-support-levels.md). The",
        "procurement view, including parts with no board package, is",
        "[Supported Boards](supported-boards.md).",
        "",
        "## RTOS / platform source pins",
        "",
        "`nros setup <board>` fetches exactly these (shared store",
        "`${NROS_HOME:-~/.nros}/sdk`):",
        "",
        "| Component | Pinned version | Used by |",
        "|---|---|---|",
        f"| FreeRTOS kernel | `{v(ver,'freertos-kernel')}` | FreeRTOS boards (QEMU MPS2-AN385, overlays) |",
        f"| lwIP | `{v(ver,'lwip')}` | FreeRTOS networking |",
        f"| ThreadX | `{v(ver,'threadx')}` | ThreadX (Linux sim + QEMU RISC-V64) |",
        f"| NetX Duo | `{v(ver,'threadx-netxduo')}` | ThreadX networking |",
        f"| NuttX kernel | `{v(ver,'nuttx-kernel')}` | NuttX (QEMU armv7a / rv32) |",
        # Maintainer ruling 2026-11-12 (issue 2389): the macOS floor that comes
        # with the SDK is ACCEPTED, and is stated beside the pin without
        # contradicting install.md -- nano-ros itself runs on Linux hosts only.
        f"| Zephyr SDK | `{v(ver,'zephyr-sdk')}` (+ `{v(ver,'zephyr-sdk-1-0-1')}` for the 4.4 rolling line); "
        f"its macOS host build needs macOS {zsdk_macos.split('.')[0]}+ (nano-ros hosts: Linux only) | Zephyr module builds |",
        f"| mbedTLS | `{v(ver,'mbedtls')}` | zenoh TLS transport |",
        "",
        "These pins are **decisions, not lags** — they move only with a",
        "reason, tested together (see",
        "[Build inside an existing CMake or vendor tree](../getting-started/vendored-tree.md)).",
        "",
        "## RMW backends — versions and interop pairing",
        "",
        "| Backend | Client side (pinned) | Host side | Pairing rule |",
        "|---|---|---|---|",
        f"| **Zenoh** | zenoh-pico `{zp_idx}` (vendored submodule) | Router = the one **ROS ships**: `ros2 run rmw_zenoh_cpp rmw_zenohd` — nano-ros deliberately vendors no router | zenoh-pico's wire protocol is stable across zenoh 1.x, so the pinned client interops with a newer ROS-shipped router |",
        f"| **XRCE-DDS** | Micro-XRCE-DDS client `{v(ver,'micro-xrce-dds-client')}` + Micro-CDR `{v(ver,'micro-cdr')}` | Agent `{v(ver,'xrce-agent')}` (installed by `nros setup … --rmw xrce`) | Client and agent are pinned independently and are NOT the same upstream release; they meet at the DDS-XRCE wire protocol |",
        f"| **Cyclone DDS** | fork `{v(ver,'cyclonedds-src')}` — upstream 0.10.5 + carried patches ([fork delta](https://github.com/NEWSLabNTU/nano-ros/blob/main/book-internals/src/rmw/cyclonedds-fork-delta.md)) | Talks RTPS directly to `rmw_cyclonedds_cpp` | Pinned to the Cyclone **ROS ships** (`ros-humble-cyclonedds` 0.10.5) — upstream 11.x is deliberately NOT a rebase target until a distro migration |",
        "",
        "Per-feature coverage across the backends:",
        "[Per-RMW Feature Matrix](rmw-feature-matrix.md).",
        "",
        "## ROS 2 editions",
        "",
        "| Edition | Feature / flag | Status | Notes |",
        "|---|---|---|---|",
        "| Humble | `ros-humble` (default) | Supported | `TypeHashNotSupported` key expressions, XCDR1. Ubuntu 22.04 only; ships `ros-humble-rmw-zenoh-cpp`. |",
        "| Iron | `ros-iron` | Supported | RIHS01 type hashes. |",
        "| Jazzy | `ros-jazzy` | Supported | RIHS01; Ubuntu 24.04; ships `ros-jazzy-rmw-zenoh-cpp`. A default Jazzy peer encodes XCDR1/FINAL, which nano-ros speaks; per-type XCDR2 (`@appendable`) is not enabled. |",
        "",
        "Pick the edition of the ROS system you interop with — mismatched",
        "editions discover nothing (different key expressions).",
        "",
        "**Supported means WIRE interop, not API parity.** nano-ros's API is",
        "compared against **Humble and Jazzy** — Jazzy from pinned upstream",
        "sources, since it cannot be installed beside Humble",
        f"([how the comparison runs]({book_pages.link(pathlib.Path(OUT), book_pages.page_file('reference/rmw-api-comparison.md'))})). Iron is not compared on its own; what it added and Jazzy kept is",
        "covered there. Jazzy's additions over Humble are each RULED, not each implemented:",
        "matched/incompatible-type events, timer call info,",
        "the small rclcpp accessors,",
        "`count_clients`/`count_services`, the client GID and",
        "`event_type_is_supported` are implemented;",
        "`BEST_AVAILABLE` QoS is implemented,",
        "resolved at creation against the discovered graph;",
        "dynamic typesupport, service introspection and the",
        "`~/get_type_description` service are declined on RAM/flash grounds;",
        "`rmw_discovery_options_t` is answered by the session locator and",
        "`localhost_only`. Type hashes (RIHS01) are on the wire today.",
        "",
        "## Toolchains",
        "",
        "| Tool | Pinned version |",
        "|---|---|",
        f"| Rust | `{channel}` channel (`rust-toolchain.toml`); pinned cross targets: {', '.join('`'+t+'`' for t in targets)}. `rustfmt` runs on nightly (nightly-only options). |",
        f"| Arm GCC (`arm-none-eabi`) | `{v(ver,'arm-none-eabi-gcc')}` |",
        f"| RISC-V GCC (`riscv-none-elf`) | `{v(ver,'riscv-none-elf-gcc')}` |",
        f"| QEMU (patched, Arm) | `{v(ver,'qemu')}` |",
        f"| Corrosion (CMake↔Cargo) | `{v(ver,'corrosion')}` (≥ 0.6.0 required — older versions share one build dir across workspace roots and break the `mixed` link) |",
        f"| OpenOCD | `{v(ver,'openocd')}` |",
        f"| sccache | `{v(ver,'sccache')}` |",
        "",
        "`-nros<N>` suffixes mark builds re-packaged (or patched) for the SDK",
        "store; the base version is the upstream release.",
        "",
        "## Related",
        "",
        "- [Install](../getting-started/installation.md) — how these get onto",
        "  a machine (`nros setup`, prebuilt-vs-source rules)",
        "- [Per-RMW Feature Matrix](rmw-feature-matrix.md)",
        "- [Board Support Levels](board-support-levels.md) (generated) ·",
        "  [Supported Boards](supported-boards.md) (procurement view)",
        "- [Build inside an existing CMake or vendor tree](../getting-started/vendored-tree.md)",
        "  — pinning and upgrading these from a company tree",
        "",
    ]
    return "\n".join(lines)


def main():
    self_test()
    content = book_pages.relink(render(), "reference/support-status.md")
    if "--check" in sys.argv:
        on_disk = open(OUT, encoding="utf8").read() if os.path.exists(OUT) else ""
        if on_disk != content:
            sys.exit("check-support-status: STALE — regenerate with "
                     "python3 scripts/gen-support-status.py and commit.")
        print("support-status OK")
        return
    with open(OUT, "w", encoding="utf8") as fh:
        fh.write(content)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
