#!/usr/bin/env python3
"""A feature-bearing fixture selector must name a crate that HAS those features.

Issue 1349, and it is the SECOND occurrence of one mistake.

`FixtureVariant::rmw(x)` and `FixtureVariant::features([..])` select a
`[[fixture]]` row by its cargo arguments -- `features = ["rmw-<x>"]`,
`no_default_features = true`. A crate with no `[features]` table in its
`Cargo.toml` can never have such a row: cargo refuses the build outright
("the package `X` does not contain this feature"), so the row is not written,
so the selector matches nothing.

What that costs is not a red. `select_row` reports

    no [[fixture]] row for <dir> with Selector { rmw: "zenoh", features:
    "rmw-zenoh", no_default_features: true }

which reads as a MISSING ROW -- so the reader adds a row, cargo refuses it, and
the reader concludes the fixture is unbuildable. Meanwhile every case that
resolver feeds becomes `[SKIPPED] fixture not built`, and a `skip!` is a panic
that bare `cargo nextest` renders FAILED. The cell reads as live failures
against a peer it never reached.

Measured twice, in the same file, twenty lines apart:

* `build_advertised_state_probe` -- found on its first run (phase-433,
  2026-09-08). `native-advertised-state-rust-cyclone-bidir` had never produced
  a verdict; with the selector corrected it passed 4/4 the same day.
* `build_qos_event_probe` -- found on its first run (phase-455, 2026-09-12).
  `native-qos-event-rust-zenoh-r2n` had never produced a verdict; with the
  selector corrected it passed live the same day.

Two occurrences of one bug is a class, and the fix for a class is a gate rather
than a third correction. The sweep this gate automates, run by hand 2026-09-12
over the nine crates a feature-bearing selector names:

    for d in packages/testing/nros-tests/bins/*/; do
        grep -q '^\\[features\\]' "$d/Cargo.toml" || echo "$d has no [features]"
    done

Both live sites are fixed; this refuses the third.

Buildless and source-only: it reads two Rust call sites and a TOML table, so it
belongs on the fast line (CLAUDE.md, the affordability-tier rule).
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RESOLVER = os.path.join(
    ROOT, "packages", "testing", "nros-tests", "src", "fixtures", "binaries", "mod.rs"
)

# A selector call spans several lines: the directory literal comes first, the
# variant a line or two later. Anchored on `select_row(` so `select_sole_row`
# -- which carries no Selector at all, and is the CORRECTION for both measured
# defects -- is not matched.
CALL_RE = re.compile(
    r"""select_row\s*\(\s*
        "(?P<dir>[^"]+)"\s*,\s*
        &(?:crate::)?(?:fixtures::)?(?:groups::)?FixtureVariant::(?P<kind>\w+)\s*\(
        (?P<args>[^)]*)""",
    re.X,
)

# `plain` and `platform_rmw` author no `features` key, so they are the shapes a
# crate without a `[features]` table CAN be selected by.
FEATURE_BEARING = ("rmw", "features")

FEATURES_TABLE_RE = re.compile(r"^\s*\[features\]\s*$", re.M)


def crate_features(manifest_path):
    """The feature names a crate declares, or None when it has no table.

    `None` and `set()` are different answers: a crate with an empty `[features]`
    table has authored the table, and the mistake this gate catches is the
    absence of one.
    """
    with open(manifest_path, encoding="utf-8") as fh:
        text = fh.read()
    m = FEATURES_TABLE_RE.search(text)
    if m is None:
        return None
    names = set()
    for line in text[m.end():].splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            break  # the next table
        key = re.match(r"([A-Za-z0-9_-]+)\s*=", stripped)
        if key:
            names.add(key.group(1))
    return names


def call_sites(text):
    """(dir, kind, args) for every feature-bearing `select_row` call."""
    out = []
    for m in CALL_RE.finditer(text):
        if m.group("kind") in FEATURE_BEARING:
            out.append((m.group("dir"), m.group("kind"), m.group("args").strip()))
    return out


def check(resolver_path=RESOLVER, root=ROOT):
    with open(resolver_path, encoding="utf-8") as fh:
        text = fh.read()
    sites = call_sites(text)
    problems = []
    for directory, kind, args in sites:
        manifest = os.path.join(root, directory, "Cargo.toml")
        if not os.path.isfile(manifest):
            problems.append(
                f"{directory}: FixtureVariant::{kind}({args}) names a directory "
                f"with no Cargo.toml"
            )
            continue
        declared = crate_features(manifest)
        if declared is None:
            problems.append(
                f"{directory}: FixtureVariant::{kind}({args}) selects a row by cargo "
                f"FEATURES, and this crate has no [features] table -- so no such row "
                f"can exist and the selector matches nothing. Every case this resolver "
                f"feeds becomes `[SKIPPED] fixture not built`, which bare nextest "
                f"renders FAILED. Use FixtureVariant::plain()/platform_rmw(), or "
                f"select_sole_row when the crate has exactly one coordinate."
            )
            continue
        if kind == "features":
            for name in re.findall(r'"([^"]+)"', args):
                if name not in declared:
                    problems.append(
                        f"{directory}: FixtureVariant::features naming {name!r}, which "
                        f"this crate does not declare (has: "
                        f"{', '.join(sorted(declared)) or 'none'})"
                    )
    return problems, len(sites)


def self_test():
    """Both directions, planted, on the normal path."""
    import tempfile

    failures = []

    def expect(name, got, want):
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    with tempfile.TemporaryDirectory() as tmp:
        good = os.path.join(tmp, "bins", "has-features")
        bad = os.path.join(tmp, "bins", "no-features")
        os.makedirs(good)
        os.makedirs(bad)
        with open(os.path.join(good, "Cargo.toml"), "w") as fh:
            fh.write('[package]\nname = "has-features"\n\n[features]\nrmw-zenoh = []\n')
        with open(os.path.join(bad, "Cargo.toml"), "w") as fh:
            fh.write('[package]\nname = "no-features"\n')

        src = os.path.join(tmp, "mod.rs")
        with open(src, "w") as fh:
            fh.write(
                'let row = select_row(\n'
                '    "bins/has-features",\n'
                '    &crate::fixtures::groups::FixtureVariant::rmw(rmw),\n'
                ');\n'
            )
        problems, n = check(src, tmp)
        expect("a feature-bearing selector on a crate WITH the table passes", problems, [])
        expect("one call site seen", n, 1)

        with open(src, "w") as fh:
            fh.write(
                'let row = select_row(\n'
                '    "bins/no-features",\n'
                '    &crate::fixtures::groups::FixtureVariant::rmw(Rmw::Zenoh),\n'
                ');\n'
            )
        problems, _ = check(src, tmp)
        expect("the measured defect is caught", len(problems), 1)
        if problems and "no [features] table" not in problems[0]:
            failures.append(f"the message does not name the cause: {problems[0]}")

        # `select_sole_row` is the correction both defects took: no Selector, so
        # nothing to disagree with. It must not be matched at all.
        with open(src, "w") as fh:
            fh.write(
                'let row = select_sole_row(\n'
                '    "bins/no-features",\n'
                ');\n'
            )
        problems, n = check(src, tmp)
        expect("select_sole_row is not a feature-bearing selector", n, 0)

        # `plain` and `platform_rmw` author no features key.
        with open(src, "w") as fh:
            fh.write(
                'let row = select_row(\n'
                '    "bins/no-features",\n'
                '    &crate::fixtures::groups::FixtureVariant::platform_rmw(rmw),\n'
                ');\n'
            )
        problems, n = check(src, tmp)
        expect("platform_rmw on a table-less crate is legal", (problems, n), ([], 0))

        # A named feature the crate does not declare.
        with open(src, "w") as fh:
            fh.write(
                'let row = select_row(\n'
                '    "bins/has-features",\n'
                '    &crate::fixtures::groups::FixtureVariant::features(&["link-tls"]),\n'
                ');\n'
            )
        problems, _ = check(src, tmp)
        expect("an undeclared named feature is caught", len(problems), 1)

    for line in failures:
        print("  " + line, file=sys.stderr)
    print(f"self-test: {len(failures)} check(s) failed")
    return 1 if failures else 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    problems, n = check()
    if n == 0:
        # Vacuity guard, and it belongs HERE rather than in `check`: a regex over
        # Rust is exactly the thing that silently stops matching, and a gate that
        # scans nothing prints the same OK as a gate that scanned everything. The
        # real resolver has had feature-bearing selectors since the shape existed;
        # zero means the pattern drifted, not that the tree got better.
        print(
            "check-fixture-variant-features: the scan found NO feature-bearing "
            "selector in " + os.path.relpath(RESOLVER, ROOT) + " -- the pattern has "
            "drifted. A gate that matches nothing passes everything.",
            file=sys.stderr,
        )
        return 1
    if problems:
        print(
            f"check-fixture-variant-features: {len(problems)} feature-bearing fixture "
            f"selector(s) name a crate that cannot have such a row:",
            file=sys.stderr,
        )
        for p in problems:
            print("  " + p, file=sys.stderr)
        return 1
    print(f"check-fixture-variant-features: OK ({n} feature-bearing selector(s) checked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
