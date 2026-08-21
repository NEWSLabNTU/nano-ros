#!/usr/bin/env python3
"""Every `binary(...)` in `.config/nextest.toml` must name a real test target.

Issue 0743's fallout. Deleting a test file leaves the nextest overrides that
filtered on it, and an override naming a missing BINARY is not inert the way a
stale test NAME is — nextest refuses to parse the config at all:

    error: for config file `.config/nextest.toml`,
           failed to parse profile.default.overrides at index 9
      error: operator didn't match any binary names

That kills EVERY nextest invocation in the repo, including ones that have
nothing to do with the deleted lane. On 2026-08-21 two deletions
(`nuttx_qemu`, `threadx_linux`) did exactly that, and it went unnoticed through
a green `just check` — because `just check` does not run nextest. This gate puts
the check where the green was.

## Why `binary()` and not `test()`

`test()` names are NOT checkable statically and this gate deliberately does not
try. The names in that file — `Platform__Nuttx`, `zenoh_rust_pubsub_e2e`,
`nuttx_riscv` — are rstest-generated CASE names; none of them appears literally
anywhere in the test sources, so any grep-based check would flag all of them.
Deriving them means compiling the workspace and running `cargo nextest list`,
which is far too heavy for `just check` and is compilation-inside-a-test
everywhere else.

That gap is real and has bitten: the file's own comments record five overrides
that selected test names phase-329 W1 had deleted, "so all five had been inert".
An inert override silently stops applying its timeout / retries / test-group.
But a stale `test()` degrades quietly and a stale `binary()` takes the whole
repo down, so this gate covers the fatal half exactly rather than the quiet half
approximately.

## Why a TOML parser and not a grep

The first pass at this audit grepped for `binary\\(...\\)` and reported
`dds_api` + `dds_ros2_interop` as dead references. They are dead — and they are
also in COMMENTS ("Phase 169.4 — `binary(dds_api)` removed (test deleted)"),
which is the correct state for a removed filter. A grep cannot tell a live
filter from prose about a dead one; a parser reads only the `filter` values.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib

BINARY_RE = re.compile(r"\bbinary\(([^)]+)\)")


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )


def known_targets(root: Path) -> set[str]:
    """Test-binary names cargo will produce, derived without compiling.

    Three sources, matching how cargo names test binaries:
      * every `<crate>/tests/<name>.rs` is an integration target `<name>`
      * an explicit `[[test]] name = "..."` overrides/adds one
      * `binary()` also matches a crate's own unit-test binary, which is named
        after the crate with `-` normalised to `_`
    """
    files = subprocess.run(
        ["git", "ls-files", "*/tests/*.rs", "tests/*.rs", "*/Cargo.toml", "Cargo.toml"],
        capture_output=True,
        text=True,
        check=True,
        cwd=root,
    ).stdout.split()

    targets: set[str] = set()
    for f in files:
        p = Path(f)
        if p.suffix == ".rs" and p.parent.name == "tests":
            targets.add(p.stem)
        elif p.name == "Cargo.toml":
            try:
                doc = tomllib.loads((root / p).read_text(encoding="utf-8"))
            except Exception:
                continue
            for t in doc.get("test", []) or []:
                if isinstance(t, dict) and "name" in t:
                    targets.add(str(t["name"]))
            pkg = doc.get("package", {})
            if isinstance(pkg, dict) and "name" in pkg:
                targets.add(str(pkg["name"]).replace("-", "_"))
    return targets


def filters(doc: dict):
    """Yield (where, filter_string) for every filter expression in the config."""
    for pname, profile in (doc.get("profile") or {}).items():
        if not isinstance(profile, dict):
            continue
        for i, ov in enumerate(profile.get("overrides") or []):
            if isinstance(ov, dict) and isinstance(ov.get("filter"), str):
                yield f"profile.{pname}.overrides[{i}]", ov["filter"]
        for i, sc in enumerate(profile.get("scripts") or []):
            if isinstance(sc, dict) and isinstance(sc.get("filter"), str):
                yield f"profile.{pname}.scripts[{i}]", sc["filter"]
        if isinstance(profile.get("default-filter"), str):
            yield f"profile.{pname}.default-filter", profile["default-filter"]


def scan(config: Path, targets: set[str]) -> list[str]:
    doc = tomllib.loads(config.read_text(encoding="utf-8"))
    bad = []
    for where, expr in filters(doc):
        for name in BINARY_RE.findall(expr):
            name = name.strip().strip("'\"")
            # `binary(=exact)` and regex forms are nextest matcher syntax; only
            # a plain name can be checked against the target list.
            if name.startswith(("=", "~", "/")):
                continue
            if name not in targets:
                bad.append(f"{where}: binary({name}) names no test target")
    return bad


SELF_TESTS = [
    ("live filter naming a real target -> ok", 'filter = "binary(rtos_e2e)"', {"rtos_e2e"}, 0),
    ("live filter naming a dead target -> flagged", 'filter = "binary(gone)"', {"rtos_e2e"}, 1),
    (
        "dead name mentioned only in a COMMENT -> ok",
        '# binary(dds_api) removed (test deleted)\nfilter = "binary(rtos_e2e)"',
        {"rtos_e2e"},
        0,
    ),
    (
        "one dead disjunct inside an or-expression -> flagged",
        'filter = "binary(gone) or (binary(rtos_e2e) and test(Nuttx))"',
        {"rtos_e2e"},
        1,
    ),
    (
        "test() names are never checked",
        'filter = "binary(rtos_e2e) and test(Platform__Nuttx)"',
        {"rtos_e2e"},
        0,
    ),
]


def self_test() -> int:
    failures = 0
    with tempfile.TemporaryDirectory() as td:
        for name, body, targets, expected in SELF_TESTS:
            f = Path(td) / "nextest.toml"
            f.write_text("[[profile.default.overrides]]\n" + body + "\n")
            got = len(scan(f, targets))
            ok = got == expected
            print(f"  [{'OK' if ok else 'FAIL'}] {name}")
            if not ok:
                failures += 1
                print(f"        expected {expected} violation(s), got {got}")
    if failures:
        print(f"\ncheck-nextest-binary-filters --self-test: {failures} case(s) FAILED")
        return 1
    print(f"\ncheck-nextest-binary-filters --self-test: {len(SELF_TESTS)} case(s) OK")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()

    root = repo_root()
    config = root / ".config/nextest.toml"
    if not config.exists():
        print(f"check-nextest-binary-filters: no {config} — nothing to check")
        return 0

    targets = known_targets(root)
    bad = scan(config, targets)
    if bad:
        print("check-nextest-binary-filters: FAIL\n")
        for b in bad:
            print(f"  {b}")
        print(
            "\nnextest REFUSES TO PARSE a config whose `binary()` names no target\n"
            "(\"operator didn't match any binary names\"), so this breaks every\n"
            "nextest run in the repo, not just the lane you deleted. Point the\n"
            "filter at the target that replaced the coverage, or delete the\n"
            "override — and if you want to keep the note, put the dead name in a\n"
            "COMMENT, which is what the removed `dds_api` filters did."
        )
        return 1
    print(f"check-nextest-binary-filters: OK ({len(targets)} test targets known)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
