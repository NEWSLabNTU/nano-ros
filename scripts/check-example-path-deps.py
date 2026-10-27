#!/usr/bin/env python3
"""Issue 2085 — an example names what it depends on; the toolchain says where
it lives. No tracked `examples/**/Cargo.toml` may reach a crate by a `path`
that leaves `examples/`.

# The rule

Every example is copy-out (RFC-0026), workspaces included since issue 2085
amended RFC-0085: a user copies the directory to start a project, and it must
build there. A path dependency into the checkout,

    nros = { path = "../../../../../packages/api/nros", ... }

is correct in the one place it was written and meaningless one directory
elsewhere: `nros sync` in the copy fails with "no matching package named
`nros` found". The portable spelling names the crate,

    nros = { version = "*", ... }

and `nros sync` writes the `[patch.crates-io]` row that resolves it into the
SDK, exactly as for every single-package example.

A path that stays inside `examples/` is not this rule's business: a sibling
workspace member, or a `generated/` message crate `nros sync` writes, travels
with the copy.

Scope: every dependency table (`[dependencies]`, `dev-`, `build-`, their
`target.<cfg>.` forms, a `[dependencies.<name>]` sub-table) and every
`[patch.<registry>]` row, because a patch into the checkout is the same
non-portable fact one table over.

Run: python3 scripts/check-example-path-deps.py [--self-test]
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 — the repo's interpreter is 3.10
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[1]
DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


def path_deps(doc: dict):
    """Yield (table, crate, path) for every `path` dependency or patch row."""

    def from_table(where: str, table) -> None:
        if not isinstance(table, dict):
            return
        for crate, spec in table.items():
            if isinstance(spec, dict) and isinstance(spec.get("path"), str):
                found.append((where, crate, spec["path"]))

    found: list[tuple[str, str, str]] = []
    for name in DEP_TABLES:
        from_table(name, doc.get(name))
    for cfg, body in (doc.get("target") or {}).items():
        if isinstance(body, dict):
            for name in DEP_TABLES:
                from_table(f"target.{cfg}.{name}", body.get(name))
    for registry, rows in (doc.get("patch") or {}).items():
        from_table(f"patch.{registry}", rows)
    return found


def offenders(root: Path, manifests: list[str]) -> list[str]:
    examples = (root / "examples").resolve()
    out = []
    for rel in manifests:
        manifest = root / rel
        try:
            doc = tomllib.loads(manifest.read_text())
        except (OSError, tomllib.TOMLDecodeError) as e:
            # check-manifests-parse owns a manifest that does not parse.
            print(f"check-example-path-deps: NOTE {rel}: not read ({e})", file=sys.stderr)
            continue
        for where, crate, path in path_deps(doc):
            target = Path(os.path.normpath(manifest.parent / path))
            if target != examples and examples not in target.parents:
                shown = os.path.relpath(target, root)
                out.append(f"{rel}: [{where}] {crate} = {{ path = \"{path}\" }}  -> {shown}")
    return out


def tracked_manifests(root: Path) -> list[str]:
    env = dict(os.environ)
    nros_clear_inherited_git_env(env)
    res = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z", "--", "examples/**/Cargo.toml", "examples/*/Cargo.toml"],
        check=True,
        capture_output=True,
        env=env,
    )
    return sorted({p for p in res.stdout.decode().split("\0") if p})


def report(found: list[str]) -> int:
    if not found:
        return 0
    print(
        f"check-example-path-deps: FAIL — {len(found)} dependenc(y/ies) reach out of "
        "`examples/` by path, so the example does not build once copied out (issue 2085):",
        file=sys.stderr,
    )
    for line in found:
        print(f"  {line}", file=sys.stderr)
    print(
        "\n  Name the crate instead — `<crate> = { version = \"*\", ... }` — and let\n"
        "  `nros sync` write the `[patch.crates-io]` row that resolves it (RFC-0098 D1).",
        file=sys.stderr,
    )
    return 1


def self_test(quiet: bool = False) -> int:
    cases = {
        # (manifest body, expected offender count)
        "into-checkout": ('[dependencies]\nnros = { path = "../../../../packages/api/nros" }\n', 1),
        "by-version": ('[dependencies]\nnros = { version = "*" }\n', 0),
        "sibling-member": ('[dependencies]\nother = { path = "../other" }\n', 0),
        "generated": ('[dependencies]\nstd_msgs = { path = "../../generated/std_msgs" }\n', 0),
        "target-scoped": (
            "[target.'cfg(target_os = \"none\")'.dependencies]\n"
            'board = { path = "../../../../packages/boards/b" }\n',
            1,
        ),
        "sub-table": ('[dependencies.nros]\npath = "../../../../packages/api/nros"\n', 1),
        "dev": ('[dev-dependencies]\nt = { path = "../../../../packages/testing/t" }\n', 1),
        "patch": ('[patch.crates-io]\nnros = { path = "../../../../packages/api/nros" }\n', 1),
        "outside-repo": ('[dependencies]\nx = { path = "../../../../../elsewhere" }\n', 1),
    }
    failed = 0
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        for name, (body, want) in cases.items():
            rel = f"examples/ws-{name}/src/pkg/Cargo.toml"
            (root / rel).parent.mkdir(parents=True)
            (root / rel).write_text('[package]\nname = "p"\nversion = "0.1.0"\n' + body)
            got = len(offenders(root, [rel]))
            if got != want:
                print(f"check-example-path-deps self-test: {name}: {got} offender(s), want {want}", file=sys.stderr)
                failed += 1
    if failed:
        return 1
    if not quiet:
        print(f"check-example-path-deps self-test: OK ({len(cases)} cases, both directions)")
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment, and this rule's whole job is to fire.
    if self_test(quiet=True):
        return 2
    manifests = tracked_manifests(REPO)
    if not manifests:
        print("check-example-path-deps: FAIL — found no tracked example manifest; refusing to report OK over nothing", file=sys.stderr)
        return 2
    rc = report(offenders(REPO, manifests))
    if rc == 0:
        print(f"check-example-path-deps: OK — {len(manifests)} example manifest(s), none reaches out of `examples/` by path")
    return rc


if __name__ == "__main__":
    sys.exit(main())
