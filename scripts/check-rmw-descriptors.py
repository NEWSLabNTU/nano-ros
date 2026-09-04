#!/usr/bin/env python3
"""phase-347 W3 — the RMW descriptors are well-formed and unambiguous.

W2 shipped this as an AGREEMENT check: descriptors on one side, the generated
`nros_rmw_dispatch()` on the other, asserting they matched while both existed.
W3 made the descriptors the source — `cargo-nano-ros/build.rs` generates the
Rust table from them, and the cmake dispatch is generated from that — so
comparing the two is now comparing a thing to itself. **A gate that cannot fail
is worse than no gate**, so its job changed rather than its file being kept
around green.

It now checks what can still go wrong once there is a single source:

  S1  every descriptor has the NON-DERIVABLE fields the generators read, and
      no empty ones that would silently lower to nothing;
  S2  no two backends claim the same name (ambiguous resolution);
  S4  a descriptor exists for every backend directory that looks like one, so
      adding a backend and forgetting the descriptor fails here rather than at
      a consumer's link.

phase-420 W5 (RFC-0087 D4) retired two of the original five rules, because
what they guarded stopped being writable:

  * S1 checked `cargo_feature` / `cmake_value` / `c_define_token` /
    `cffi_feature` were present and non-empty. All four are DERIVED now
    (`cargo-nano-ros/src/derived_descriptor.rs`), so requiring them would
    require the duplication the wave deleted. `cpp_define` is what is left —
    the one lowering convention cannot produce, since the spellings are
    inconsistent across backends by history and consumers `#if` on them.
  * S3 asserted the canonical name was `names[0]`. `names` is gone from the
    descriptors; `build.rs` takes the FIRST `<nano_ros_provides>` announcement
    as canonical, so "the canonical name is the first one" is now structural
    rather than checkable.

**Names are read from the `package.xml` announcements here too**, for the same
reason: after W5 that is where they live. `check-provider-announcements` A2n
refuses a descriptor that restates them, so S2 cannot be looking at a stale
second copy.

Agreement between a descriptor and its `package.xml` provisions (phase-348) was
briefly S5 here. It moved to `scripts/check-provider-announcements.py` when
boards needed the same rule: one gate covering every provider family beats a
copy of the rule next to each family's descriptors, which is the second-spelling
antipattern that turned #282 into #326. What stays here is what is rmw-SPECIFIC.

`build.rs` also panics on S1/S2, deliberately — that is the belt to this gate's
braces, and it fires for anyone who builds rather than only for `check-fast`.
This runs buildless so a malformed descriptor is caught before a cryptic build
failure.
"""

import glob
import os
import re
import sys

try:
    import tomllib  # 3.11+
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Fields the generators read that convention CANNOT produce. An empty value
# lowers to nothing silently, so an empty string is as much a failure as a
# missing key. (The four derivable ones left this list in phase-420 W5; the
# gate for those is `check-derived-descriptor-fields`, which asserts a
# RESTATEMENT equals its derived value rather than asserting presence.)
REQUIRED = ["cpp_define"]

PROVIDES_RE = re.compile(r'<nano_ros_provides\s+kind="rmw"\s+name="([^"]+)"\s*/?>')
# An XML comment body cannot contain `--`, so this matches one exactly. The
# strip is not optional (issue 0516): every backend package.xml documents the
# tag in a comment above the real one.
COMMENT_RE = re.compile(r"<!--([^-]|-[^-])*-->")


def announced_names(desc_path):
    """The backend's names, from the `package.xml` beside its descriptor.

    Empty when the package.xml is absent, which is not an error here: an
    unmigrated provider is simply not discoverable while its existing build
    path keeps working (the same allowance `check-provider-announcements`
    makes).
    """
    pkg_xml = os.path.join(os.path.dirname(desc_path), "package.xml")
    if not os.path.exists(pkg_xml):
        return []
    with open(pkg_xml, encoding="utf-8") as fh:
        return PROVIDES_RE.findall(COMMENT_RE.sub("", fh.read()))


def main():
    paths = sorted(glob.glob(os.path.join(ROOT, "packages/rmw/*/*/nros-rmw.toml")))
    if not paths:
        sys.exit(
            "check-rmw-descriptors: no nros-rmw.toml found — refusing to pass on "
            "an empty set (the gate would be vacuous)"
        )

    problems = []
    claimed = {}

    for path in paths:
        rel = os.path.relpath(path, ROOT)
        with open(path, "rb") as fh:
            try:
                data = tomllib.load(fh)
            except Exception as e:  # noqa: BLE001 — report, do not raise
                problems.append(f"{rel}: not valid TOML: {e}")
                continue
        rmw = data.get("rmw")
        if not rmw:
            problems.append(f"{rel}: no [rmw] table")
            continue

        names = announced_names(path)
        if not names:
            problems.append(
                f"{rel}: the package.xml beside it announces no "
                f'<nano_ros_provides kind="rmw"/> — nothing could resolve to it '
                f"(phase-420 W5: the announcement is where a backend's names live)"
            )
            continue

        # S1
        for field in REQUIRED:
            if not rmw.get(field):
                problems.append(f"{rel}: [rmw].{field} is missing or empty")

        # S2
        for n in names:
            if n in claimed:
                problems.append(
                    f"two descriptors claim rmw name {n!r}: {claimed[n]} and {rel}"
                )
            claimed[n] = rel

    # S4 — a backend directory with no descriptor. A backend is identified by
    # shipping sources under a `packages/rmw/<family>/<pkg>/src`; the support
    # crates (cffi, bridge, metadata, transport-callbacks) are not backends and
    # are listed here rather than guessed.
    NOT_BACKENDS = {"cffi", "bridge", "metadata", "transport-callbacks"}
    for family in sorted(glob.glob(os.path.join(ROOT, "packages/rmw/*"))):
        fam = os.path.basename(family)
        if not os.path.isdir(family) or fam in NOT_BACKENDS:
            continue
        if not glob.glob(os.path.join(family, "*", "nros-rmw.toml")):
            problems.append(
                f"packages/rmw/{fam}: looks like a backend family but ships no "
                f"nros-rmw.toml — add one, or add {fam!r} to NOT_BACKENDS in "
                f"{os.path.basename(__file__)} saying why it is not a backend"
            )

    if problems:
        sys.stderr.write("check-rmw-descriptors: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1

    print(
        f"rmw descriptors: OK ({len(paths)} descriptor(s), "
        f"{len(claimed)} name(s) claimed, no duplicates)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
