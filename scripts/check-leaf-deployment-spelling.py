#!/usr/bin/env python3
"""A single-package leaf states its deployment in `system.toml` — phase-445 W3b.

THE RULE (RFC-0098 D3/D5). A single-package example — a package that is its own
image, not a member of a workspace — names its board, RMW, domain, network
identity and node in the `system.toml` beside its manifest. The two spellings
that preceded it are REFUSED:

  R1  a leaf `Cargo.toml` carrying `[package.metadata.nros.deploy.<board>]`,
      `[package.metadata.nros.node]`, `[package.metadata.nros.component]`, or a
      `[package.metadata.nros.entry]` that holds anything but the build facts
      that are not deployment (`node_pkgs`, `max_callbacks`,
      `max_sched_contexts` — the RTIC leaves keep those);
  R2  a `<nano_ros deploy= board= rmw=/>` element in ANY tracked `package.xml`
      (comments stripped, as every package.xml reader must — issue 0516). The
      cmake reader refuses it at configure time; this refuses it at push time,
      before a configure is needed to find out.

WHY A GATE. The conversion was mechanical over 133 leaves, and both old
spellings are what every copied-out example and every older doc shows. A leaf
re-growing a `[package.metadata.nros.deploy.*]` table still BUILDS — the
deprecated reader (`leaf_system::from_manifest`) answers for it with a warning
nobody reads — so nothing else would notice the drift back.

WHAT IS NOT A SINGLE-PACKAGE LEAF (and so not R1's business yet). Workspace
members and entries — `examples/workspaces/**`, `examples/templates/**`, and
the workspace fixtures under `packages/testing/nros-tests/fixtures/**` and
`packages/cli/**/tests/fixtures/**` — still carry the retiring keys, and the
deprecated reader stays for them until phase-445 W4/W5 moves workspaces under
`build/`. They are listed below as WORKSPACE_ROOTS and REPORTED as skipped,
never silently. R2 has no such carve-out: no package.xml anywhere may carry
the tuple, because the reader that understood it is gone.

Discovery is the git index (`scripts/lib/tracked.py`, issue 0721), with the
inherited repository environment cleared first (issue 0986) — this runs in the
fast lane the `pre-push` hook reaches, where an inherited `GIT_DIR` names
another index. `--self-test` runs on the normal path too, against a temp tree.

Exit 0 when no leaf carries a retired spelling, 1 otherwise.
"""

import re
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402
from tracked import tracked  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

# Workspace-shaped trees: members and entries whose deployment moves with the
# workspace restructuring (phase-445 W4/W5), not with this wave.
WORKSPACE_ROOTS = (
    "examples/workspaces/",
    "examples/templates/",
    "packages/testing/nros-tests/fixtures/",
    "packages/cli/",
)

# `[package.metadata.nros.entry]` keys that are build facts, not deployment.
ENTRY_KEEP = {"node_pkgs", "max_callbacks", "max_sched_contexts"}

# `<nano_ros` followed by whitespace: NOT `<nano_ros_uses` / `<nano_ros_provides`.
TUPLE = re.compile(r"<nano_ros[ \t\r\n][^>]*>")
COMMENT = re.compile(r"<!--([^-]|-[^-])*-->")


def manifest_problems(text):
    """Retired deployment tables in one Cargo.toml's text, by name."""
    try:
        doc = tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        return [f"unparseable ({e})"]
    nros = doc.get("package", {}).get("metadata", {}).get("nros", {})
    out = []
    for key in sorted(nros.get("deploy", {})):
        out.append(f"[package.metadata.nros.deploy.{key}]")
    for key in ("node", "component"):
        if key in nros:
            out.append(f"[package.metadata.nros.{key}]")
    extra = sorted(set(nros.get("entry", {})) - ENTRY_KEEP)
    if extra:
        out.append("[package.metadata.nros.entry] " + ", ".join(extra))
    return out


def package_xml_problems(text):
    return TUPLE.findall(COMMENT.sub("", text))


def scan(root, repo=None):
    """(problems, skipped_workspace_manifests, leaves_checked, xml_checked)."""
    root = Path(root)
    problems, skipped, leaves, xmls = [], 0, 0, 0
    for p in tracked(root / "examples", root / "packages", name="Cargo.toml", repo=repo):
        rel = p.relative_to(root).as_posix()
        if any(rel.startswith(w) for w in WORKSPACE_ROOTS):
            if manifest_problems(p.read_text(encoding="utf8")):
                skipped += 1
            continue
        # A leaf is a package with a package.xml beside its manifest — the
        # shape `nros sync`'s single-package mode keys on. Board and driver
        # crates carry no package.xml deployment and are not images.
        if not (p.parent / "package.xml").is_file():
            continue
        leaves += 1
        for why in manifest_problems(p.read_text(encoding="utf8")):
            problems.append(f"{rel}: {why} — state it in {Path(rel).parent}/system.toml")
    for p in tracked(root, name="package.xml", repo=repo):
        rel = p.relative_to(root).as_posix()
        xmls += 1
        for tag in package_xml_problems(p.read_text(encoding="utf8", errors="replace")):
            problems.append(
                f"{rel}: {tag} — the tuple is retired; state the deployment in "
                f"{Path(rel).parent}/system.toml (`[image.<id>] board`, `[system] rmw`)"
            )
    return problems, skipped, leaves, xmls


def self_test():
    ok_manifest = '[package]\nname = "x"\n[package.metadata.nros.entry]\nnode_pkgs = ["x"]\n'
    assert manifest_problems(ok_manifest) == [], manifest_problems(ok_manifest)
    bad = (
        '[package]\nname = "x"\n[package.metadata.nros.entry]\ndeploy = "freertos"\n'
        "[package.metadata.nros.node]\nclass = \"x::X\"\n"
        '[package.metadata.nros.deploy.freertos]\nip = "1.2.3.4"\n'
    )
    got = manifest_problems(bad)
    assert "[package.metadata.nros.entry] deploy" in got, got
    assert "[package.metadata.nros.node]" in got, got
    assert "[package.metadata.nros.deploy.freertos]" in got, got
    assert package_xml_problems('<export><nano_ros deploy="native"/></export>'), "tuple missed"
    assert package_xml_problems('<!-- <nano_ros deploy="native"/> --><export/>') == []
    assert package_xml_problems('<nano_ros_uses kind="rmw" name="zenoh"/>') == []
    assert package_xml_problems('<nano_ros_provides kind="board" name="x"/>') == []

    # End to end over a temp tree (walked, not indexed: it is outside the repo).
    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        leaf = t / "examples" / "native" / "rust" / "talker"
        leaf.mkdir(parents=True)
        (leaf / "package.xml").write_text("<package><export/></package>\n")
        (leaf / "Cargo.toml").write_text(bad)
        cxx = t / "examples" / "native" / "c" / "talker"
        cxx.mkdir(parents=True)
        (cxx / "package.xml").write_text('<package><export><nano_ros deploy="native"/></export></package>\n')
        ws = t / "examples" / "workspaces" / "w" / "src" / "entry"
        ws.mkdir(parents=True)
        (ws / "package.xml").write_text("<package/>\n")
        (ws / "Cargo.toml").write_text(bad)
        problems, skipped, leaves, xmls = scan(t)
        assert leaves == 1 and skipped == 1 and xmls == 3, (leaves, skipped, xmls)
        assert sum("rust/talker/Cargo.toml" in p for p in problems) == 3, problems
        assert sum("c/talker/package.xml" in p for p in problems) == 1, problems
        # The negative control: fixed, the same tree is clean.
        (leaf / "Cargo.toml").write_text(ok_manifest)
        (cxx / "package.xml").write_text("<package><export/></package>\n")
        problems, _, _, _ = scan(t)
        assert problems == [], problems
    sys.stdout.write("check-leaf-deployment-spelling self-test: OK\n")


def main():
    nros_clear_inherited_git_env()
    self_test()
    if "--self-test" in sys.argv:
        return 0
    problems, skipped, leaves, xmls = scan(ROOT)
    if leaves == 0 or xmls == 0:
        sys.stderr.write(
            "check-leaf-deployment-spelling: found no leaf manifest / package.xml —\n"
            "this gate would pass vacuously; check the tree or the index.\n"
        )
        return 1
    if problems:
        sys.stderr.write(
            "check-leaf-deployment-spelling: %d retired deployment spelling(s) "
            "(RFC-0098 D3/D5, phase-445 W3b)\n\n" % len(problems)
        )
        for p in problems:
            sys.stderr.write("  - %s\n" % p)
        return 1
    sys.stdout.write(
        "check-leaf-deployment-spelling: OK — %d single-package leaf manifest(s) and "
        "%d package.xml carry no retired deployment spelling; %d workspace-shaped "
        "manifest(s) still on it SKIPPED (phase-445 W4/W5 owns them).\n"
        % (leaves, xmls, skipped)
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
