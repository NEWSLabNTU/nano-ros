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

  S1  every descriptor has the fields the generators read, and no empty ones
      that would silently lower to nothing;
  S2  no two descriptors claim the same name (ambiguous resolution);
  S3  the canonical name is the FIRST entry of `names`, because that is what
      `declared` becomes and what error messages list;
  S4  a descriptor exists for every backend directory that looks like one, so
      adding a backend and forgetting the descriptor fails here rather than at
      a consumer's link;
  S5  where a backend also carries a `package.xml` (phase-348 W1, discovery),
      its `<nano_ros_provides kind="rmw"/>` names match `[rmw].names` exactly,
      canonical first.

S5 exists because the migration creates a SECOND spelling of the name set: the
descriptor says what the backend lowers to, the package.xml says what it is
discoverable as, and nothing structural keeps them equal. It has already earned
itself — the first package.xml written claimed a `zenoh-pico` alias the
descriptor does not have, which would have made discovery and resolution
disagree about which names exist. Packages without a package.xml are simply not
checked, so the W2 migration can proceed one provider at a time.

`build.rs` also panics on S1/S2, deliberately — that is the belt to this gate's
braces, and it fires for anyone who builds rather than only for `check-fast`.
This runs buildless so a malformed descriptor is caught before a cryptic build
failure.
"""

import glob
import re
import os
import sys

try:
    import tomllib  # 3.11+
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Fields the generators read. An empty value lowers to nothing silently, so an
# empty string is as much a failure as a missing key.
REQUIRED = ["cargo_feature", "cmake_value", "c_define_token", "cffi_feature", "cpp_define"]


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

        names = rmw.get("names") or []
        if not names:
            problems.append(f"{rel}: [rmw].names is empty — nothing could resolve to it")
            continue

        # S1
        for field in REQUIRED:
            if not rmw.get(field):
                problems.append(f"{rel}: [rmw].{field} is missing or empty")

        # S3 — the canonical name is names[0]; a bare name that is not first
        # means `declared` would be an alias, and error messages would list it.
        canonical = names[0]
        if rmw.get("cmake_value") and rmw["cmake_value"] != canonical:
            problems.append(
                f"{rel}: cmake_value {rmw['cmake_value']!r} != names[0] {canonical!r} — "
                f"the canonical name must be the first entry"
            )

        # S5 — discovery (package.xml) and resolution (descriptor) must claim
        # the same names. Parsed with a regex rather than an XML library
        # because this gate must run buildless with no third-party import, and
        # the element is fixed-shape; the CLI's `quick_xml` reader is the real
        # parser and this only has to detect DISAGREEMENT.
        pkg_xml = os.path.join(os.path.dirname(path), "package.xml")
        if os.path.exists(pkg_xml):
            with open(pkg_xml, encoding="utf-8") as fh:
                body = fh.read()
            # Strip XML comments first, for the same reason
            # `nros_read_package_xml_body()` does in cmake: a provider's
            # package.xml documents the provision tag in a comment, and a
            # regex cannot tell that from a declaration. Without this, a
            # commented-out example counts as a claimed name.
            body = re.sub(r"<!--([^-]|-[^-])*-->", "", body)
            declared = re.findall(
                r'<nano_ros_provides\s+kind="rmw"\s+name="([^"]+)"\s*/?>', body
            )
            if not declared:
                problems.append(
                    f"{os.path.relpath(pkg_xml, ROOT)}: sits beside an rmw descriptor "
                    f"but announces no <nano_ros_provides kind=\"rmw\"/> — it would "
                    f"be invisible to the phase-348 scan"
                )
            elif declared != names:
                problems.append(
                    f"{os.path.relpath(pkg_xml, ROOT)}: provides {declared} but "
                    f"{rel} declares names {names} — discovery and resolution must "
                    f"claim the same names, canonical first"
                )

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
