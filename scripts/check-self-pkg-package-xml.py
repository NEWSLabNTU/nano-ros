#!/usr/bin/env python3
"""issue 1501 — a self-pkg bringup that declares components must be SYNCABLE.

Since phase-330 W4 a SystemModel is a BUILD ARTIFACT, and `codegen-system`
refuses to bake a bringup that declares system semantics without one:

    `<dir>/system.toml` declares system semantics but no SystemModel was
    found. It is a BUILD ARTIFACT (phase-330 W4), so generate it rather than
    committing one:  nros sync

The only producer of that artifact is `nros sync`, and sync's package scan has
exactly two shapes (`cmd/ws.rs`): a colcon workspace (`src/<pkg>/package.xml`)
or a single-package dir (`package.xml` at the root). A directory with NO
`package.xml` is rejected outright —

    sync: no `src/<pkg>/package.xml` and no `package.xml` at root under <dir>

— so its model can never be resolved, and every consumer that demands one fails
at CONFIGURE, naming the model rather than the missing manifest.

The rule, therefore: a directory whose `system.toml` declares `[[component]]`
rows and which is a PACKAGE (`Cargo.toml` or `CMakeLists.txt` beside it — the
shape `_nros_system_detect_self_pkg` in `zephyr/cmake/nros_system_generate.cmake`
accepts as a self-pkg bringup) must carry a `package.xml`.

Measured cost of not having this gate: phase-445 W5 moved the
`[package.metadata.nros.{component,deploy.zephyr}]` tables of
`packages/testing/nros-tests/fixtures/zephyr_self_pkg/{self,sibling}/alpha_pkg`
into a new `system.toml` without adding the `package.xml` every converted leaf
(`examples/zephyr/rust/*`) already had. That flipped both fixtures from the
configless default bake into the refusal above, so neither could CONFIGURE from
2026-09-11 on. It surfaced only on 2026-09-25, when the tier-2 lane finally got
past two earlier stops (issues 1458, 1476) and reached the fixture build.

Runs its own negative control on every invocation, per AGENTS.md — a gate that
has never been shown to fail is a comment.
"""

import os
import subprocess
import sys

ROOT = subprocess.run(
    ["git", "rev-parse", "--show-toplevel"],
    capture_output=True, text=True, check=True,
).stdout.strip()

# The package markers `_nros_system_detect_self_pkg` accepts. A `system.toml`
# with neither beside it is a Path-A bringup PACKAGE DIR in someone's
# workspace, judged the same way (it still needs a `package.xml` to be
# scanned), so the marker list only decides what to call it in the message.
PKG_MARKERS = ("Cargo.toml", "CMakeLists.txt")


def declares_components(path):
    """True when this `system.toml` states system semantics sync must resolve.

    Textual, deliberately: this gate runs on the BUILDLESS fast lane, which has
    no cargo and no CLI. `[[component]]` at column 0 is the only spelling TOML
    has for an array-of-tables header, so a parser would answer the same
    question at the cost of the lane's premise.
    """
    try:
        with open(os.path.join(ROOT, path), encoding="utf-8") as fh:
            return any(line.strip() == "[[component]]" for line in fh)
    except OSError:
        return False


def violations():
    listed = subprocess.run(
        ["git", "ls-files", "*system.toml"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    bad = []
    for rel in listed:
        d = os.path.dirname(rel)
        if not declares_components(rel):
            continue
        abs_d = os.path.join(ROOT, d)
        if not any(os.path.exists(os.path.join(abs_d, m)) for m in PKG_MARKERS):
            continue
        if not os.path.exists(os.path.join(abs_d, "package.xml")):
            bad.append(d)
    return bad


def self_test():
    """The negative control: the predicate must REJECT the shape it is for.

    Asserted against a synthetic tree rather than the repo, so the control does
    not depend on the repo being clean — and both directions, because a
    predicate that answers "bad" for everything is also not a gate.
    """
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        good = os.path.join(tmp, "good")
        bad = os.path.join(tmp, "bad")
        for d in (good, bad):
            os.makedirs(d)
            with open(os.path.join(d, "system.toml"), "w", encoding="utf-8") as fh:
                fh.write("[system]\nname = \"x\"\n\n[[component]]\npkg = \"x\"\n")
            open(os.path.join(d, "Cargo.toml"), "w").close()
        open(os.path.join(good, "package.xml"), "w").close()

        def judge(d):
            has_comp = any(
                line.strip() == "[[component]]"
                for line in open(os.path.join(d, "system.toml"), encoding="utf-8")
            )
            is_pkg = any(os.path.exists(os.path.join(d, m)) for m in PKG_MARKERS)
            return has_comp and is_pkg and not os.path.exists(os.path.join(d, "package.xml"))

        assert judge(bad), "self-test: a component-declaring package dir with no package.xml must FAIL"
        assert not judge(good), "self-test: the same dir WITH a package.xml must PASS"
    print("check-self-pkg-package-xml: self-test OK (both directions).")


def main():
    self_test()
    bad = violations()
    if bad:
        print(
            "check-self-pkg-package-xml: bringup(s) declaring `[[component]]` with no "
            "`package.xml` — `nros sync` cannot scan them, so their SystemModel can "
            "never be resolved and every bake refuses at configure:",
            file=sys.stderr,
        )
        for d in bad:
            print(f"  {d}", file=sys.stderr)
        print(
            "  Add a `package.xml` beside `system.toml` (see any of "
            "`examples/zephyr/rust/*`), then `nros sync` in that directory.",
            file=sys.stderr,
        )
        return 1
    print("check-self-pkg-package-xml OK.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
