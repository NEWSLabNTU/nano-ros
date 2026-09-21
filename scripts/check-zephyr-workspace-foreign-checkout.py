#!/usr/bin/env python3
"""A Zephyr west workspace must not name ANOTHER nano-ros checkout — issue 1387.

Measured 2026-09-18, in a second checkout used because ROS 2 lives in a
distrobox::

    $ ls -l zephyr-workspace/nano-ros
    zephyr-workspace/nano-ros -> /mnt/.../nano-ros          # the OTHER checkout

So `west build` there compiled the APPLICATION from this checkout and the
nano-ros Zephyr MODULE — the zpico C shim, the platform/board headers, the RMW
dispatch dir, the knob inventory — from another one, on another branch,
silently, for a month. The `Includes:` line of the configure and the
`_NROS_RMW_DISPATCH_DIR` / `_NROS_MESSAGE_BOUNDS_DIR` / `_NROS_KNOB_INVENTORY_FILE`
entries of every `CMakeCache.txt` in that workspace all resolved into the other
tree. A mixed image BUILDS, links and runs; the two halves only disagree when
one of them changes, which is why nothing said so.

## The rule is issue 1280's, applied to workspace DATA instead of to `$ENV`

    a path OUTSIDE any nano-ros checkout  -> KEEP.  A real out-of-tree tool or
                                             SDK; this is legitimate.
    a path inside THIS checkout           -> KEEP.  Nothing to decide.
    a path inside a DIFFERENT checkout    -> FAIL.  One image, two trees.

and "which checkout" is the MARKER WALK, never `.git` — a linked worktree's
`.git` is a FILE (issue 1336) and `git rev-parse` answers about the caller's
repository rather than about an arbitrary path. The marker is READ from
`scripts/lib/checkout-paths.sh` rather than restated here: issue 1280's gate
refuses a fourth spelling of it, and a gate that introduces one would be the
defect it exists to catch.

## The three subjects, and why each is measured rather than assumed

1. **The west manifest project** (`<ws>/<[manifest] path>`) — the thing that was
   actually wrong. West lists it as the `nros` module, so it decides which
   tree's `zephyr/`, platform sources and headers every image in that workspace
   compiles.

2. **Every `<ws>/build*/CMakeCache.txt` value** — the DURABLE half. Repairing
   the link does not repair the images already built against it: the module
   root is a configure-time identity and the cached `_NROS_*_DIR` entries are
   exactly what a reconfigure would be asked to correct, so issue 1387's own
   remedy is a pristine build, not a reconfigure. An artifact built from two
   trees is not a result about either, and a gate that only checked the link
   would call such a tree clean.

3. **The venv console scripts' shebangs** (`scripts/zephyr/.venv/bin/*`) — the
   second, smaller crossing 1387 records: the box checkout's own `west` carried
   `#!/mnt/.../nano-ros/scripts/zephyr/.venv/bin/python3`, the OTHER checkout's
   interpreter, so `WEST_PYTHON` and `_Python3_EXECUTABLE` in every Zephyr
   `CMakeCache.txt` pointed there. Benign today — a build driver, not a
   compiler — and it SURVIVES the symlink repair, which is the reason it is
   checked at its source instead of only downstream in subject 2.

## Where it runs, and what it does when there is nothing to look at

The fast line. It compiles nothing and runs no west command: it reads one
config file, one glob of caches, and a directory of first lines.

With no workspace and no venv provisioned it exits **78**, and the `just`
recipe turns that into a `nros_check_skip` entry rather than an OK — a gate
that passes silently when its subject is absent is the failure mode this repo
keeps filing (issue 0650). On a CI runner with nothing provisioned that is the
normal outcome and it is SAID rather than implied.

`NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS=1` silences subject 2 ONLY, and say why —
the `NROS_ALLOW_SUBMODULE_REWIND` shape. It exists because subject 2 is a
statement about ARTIFACTS rather than about the tree: a checkout carrying a
month of images built against a foreign module is red for a reason unrelated to
the diff in front of it, and the repair is a pristine rebuild of every affected
leaf (69 of 69 in the measured case), which is not a thing to demand mid-review.
Subjects 1 and 3 are CONFIGURATION, cost an `ln`/a venv recreate, and stay
fail-closed: nothing silences them.

The manifest half is also reached from `check-zephyr-workspace-checkout.sh`
(`--manifest-only`), which front-runs the phase-431 W1 CLI ownership guard for
`check-tier-preconditions` and the `just ci` tiers. One implementation, two
callers: that script used to carry its own copy of the walk.

Run::

    check-zephyr-workspace-foreign-checkout.py [--workspace DIR] [--manifest-only]
    check-zephyr-workspace-foreign-checkout.py --self-test
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SHELL_LIB = ROOT / "scripts/lib/checkout-paths.sh"
WS_RESOLVER = ROOT / "scripts/lib/zephyr-workspace.sh"

EXIT_SKIP = 78

# How many findings of one kind to print before summarising. A mixed workspace
# has ~70 caches x ~10 crossed variables; naming all 700 buries the remedy.
MAX_SHOWN = 8


# --------------------------------------------------------------------------
# The rule
# --------------------------------------------------------------------------


def checkout_marker() -> str:
    """The one checkout marker, read from the shell library that owns it.

    Not a constant here: issue 1280's `check-inherited-checkout-paths` asserts
    that the three existing spellings agree and refuses a fourth.
    """
    m = re.search(r'NROS_CHECKOUT_MARKER="([^"]+)"', SHELL_LIB.read_text())
    if not m:
        raise SystemExit(
            f"{SHELL_LIB}: no NROS_CHECKOUT_MARKER — this gate reads the marker "
            "from there rather than restating it (issue 1280)"
        )
    return m.group(1)


def checkout_root(value: str, marker: str) -> str | None:
    """The nano-ros checkout `value` belongs to, or None.

    Lexical, like `nros_checkout_root`: a relative path answers nothing (it is
    resolved against the caller's own cwd, so it cannot have been inherited),
    and the path itself need not exist.
    """
    if not value.startswith("/"):
        return None
    d = value
    while d and d != "/":
        if os.path.isfile(os.path.join(d, marker)):
            return d
        d = d.rsplit("/", 1)[0]
    return None


def _real(path: str) -> str:
    return os.path.realpath(path)


def foreign_owner(value: str, here: str, marker: str) -> str | None:
    """The OTHER checkout `value` names, or None when the rule says KEEP.

    BOTH the spelling and its physical target are classified, because for
    different subjects a different one of the two carries the evidence:

      * the manifest project is a link INSIDE this checkout pointing at another
        one, so only the resolved target is foreign;
      * a venv console script's shebang is the opposite — measured on the box
        checkout, `#!/<other>/scripts/zephyr/.venv/bin/python3` resolves to
        `/usr/bin/python3`, OUTSIDE any checkout, so realpath alone reports
        nothing while the crossing is exactly the spelling that runs and the
        one that lands in `WEST_PYTHON`. Checking only the target missed all
        five of that venv's crossed scripts.

    `here` and the owner are resolved physically before comparison: a checkout
    reached through a symlinked parent is the same tree under a second name,
    and comparing spellings would call it foreign (issue 0375 — the resolver
    really does hand this gate `/home/<user>/data/...` for a tree whose
    physical path is `/mnt/...`).
    """
    here_real = _real(here)
    for candidate in (value, _real(value)):
        owner = checkout_root(candidate, marker)
        if owner is None:
            continue  # outside any checkout — a real out-of-tree tool. KEEP.
        if _real(owner) == here_real:
            continue  # this checkout. KEEP.
        return _real(owner)
    return None


# --------------------------------------------------------------------------
# The subjects
# --------------------------------------------------------------------------


def resolve_workspace(here: str) -> str | None:
    """The workspace THE BUILD would use — the one resolver (phase-440 W1).

    Never a fourth ladder: `just zephyr`, the west fixture builder and
    `check-tier-preconditions` all go through this file, and the three
    hand-written ladders it replaced did not agree.
    """
    p = subprocess.run(
        ["bash", str(WS_RESOLVER), "--root", here, "--absolute", "resolve"],
        capture_output=True,
        text=True,
    )
    if p.returncode != 0:
        return None
    ws = p.stdout.strip()
    return ws if ws and os.path.isdir(ws) else None


def manifest_project(ws: str) -> str | None:
    """`<ws>/<[manifest] path>` from `.west/config`, or None."""
    cfg = os.path.join(ws, ".west", "config")
    if not os.path.isfile(cfg):
        return None
    section = None
    for line in Path(cfg).read_text(errors="replace").splitlines():
        s = line.strip()
        if s.startswith("["):
            section = s.strip("[]").strip()
            continue
        if section == "manifest" and "=" in s:
            key, _, val = s.partition("=")
            if key.strip() == "path":
                rel = val.strip()
                return os.path.join(ws, rel) if rel else None
    return None


CACHE_LINE = re.compile(r"^([A-Za-z_][A-Za-z0-9_.-]*):[A-Z]+=(.*)$")


def cache_values(cache: Path) -> list[tuple[str, str]]:
    """`NAME:TYPE=VALUE` pairs whose value is an absolute path."""
    out = []
    for line in cache.read_text(errors="replace").splitlines():
        m = CACHE_LINE.match(line)
        if m and m.group(2).startswith("/"):
            out.append((m.group(1), m.group(2)))
    return out


SHEBANG_DIRS = ("scripts/zephyr/.venv/bin",)


def venv_shebangs(here: str, ws: str | None) -> list[tuple[str, str]]:
    """(script, interpreter) for every console script with an absolute shebang."""
    dirs = [os.path.join(here, d) for d in SHEBANG_DIRS]
    if ws:
        dirs.append(os.path.join(ws, ".venv", "bin"))
    out = []
    for d in dirs:
        if not os.path.isdir(d):
            continue
        for name in sorted(os.listdir(d)):
            p = os.path.join(d, name)
            if not os.path.isfile(p) or os.path.islink(p):
                continue
            try:
                with open(p, "rb") as fh:
                    first = fh.readline(512).decode("utf-8", "replace").rstrip("\n")
            except OSError:
                continue
            if not first.startswith("#!"):
                continue
            interp = first[2:].strip().split()[0] if first[2:].strip() else ""
            if interp.startswith("/"):
                out.append((p, interp))
    return out


# --------------------------------------------------------------------------
# The scan
# --------------------------------------------------------------------------


def scan(here: str, ws: str | None, marker: str, manifest_only: bool) -> tuple[list[str], int]:
    """Findings, and how many subjects were actually looked at.

    The subject COUNT is returned so the caller can tell "clean" from "there
    was nothing here" — the distinction this gate exists to make one level up.
    """
    problems: list[str] = []
    subjects = 0

    if ws:
        proj = manifest_project(ws)
        if proj and os.path.exists(proj):
            subjects += 1
            owner = foreign_owner(proj, here, marker)
            if owner:
                link = os.readlink(proj) if os.path.islink(proj) else "(not a symlink)"
                problems.append(
                    f"west manifest project names ANOTHER checkout\n"
                    f"      {proj}\n"
                    f"      -> {link}\n"
                    f"      owned by {owner}\n"
                    f"      this tree {_real(here)}\n"
                    f"    West lists the manifest project as the `nros` module, so every image\n"
                    f"    in that workspace compiles THAT tree's zephyr/, platform sources and\n"
                    f"    headers beside entry code THIS tree generates.\n"
                    f"    Fix (issue 1258 — the project is a manifest and nothing else):\n"
                    f"      bash scripts/zephyr/unbind-manifest-project.sh \\\n"
                    f"          {ws} {os.path.basename(proj)} west.yml {_real(here)}\n"
                    f"    then a PRISTINE build of anything you intend to believe."
                )

    # The hatch suppresses the FINDING, never the LOOK: a checkout whose only
    # subject is its build dirs must still count them, or silencing subject 2
    # would turn the run into a "nothing provisioned" skip.
    allow_artifacts = os.environ.get("NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS") == "1"
    if not manifest_only and ws:
        crossed: list[tuple[str, str, str]] = []
        caches = sorted(Path(ws).glob("build*/CMakeCache.txt"))
        for cache in caches:
            subjects += 1
            for name, value in cache_values(cache):
                owner = foreign_owner(value, here, marker)
                if owner:
                    crossed.append((str(cache), name, value))
        if crossed and not allow_artifacts:
            by_cache = sorted({c for c, _, _ in crossed})
            shown = crossed[:MAX_SHOWN]
            detail = "\n".join(f"      {n}={v}" for _, n, v in shown)
            more = (
                f"\n      … and {len(crossed) - len(shown)} more entries"
                if len(crossed) > len(shown)
                else ""
            )
            problems.append(
                f"{len(crossed)} cached value(s) across {len(by_cache)} of {len(caches)} "
                f"build dir(s) name ANOTHER checkout\n"
                f"      first: {by_cache[0]}\n"
                f"{detail}{more}\n"
                f"    Those images were built from two trees. The module root is a\n"
                f"    CONFIGURE-time identity and the cached `_NROS_*` entries are what a\n"
                f"    reconfigure would be asked to correct, so issue 1387's remedy is a\n"
                f"    pristine build of the affected dirs — not `cmake <build-dir>`.\n"
                f"    Until then nothing measured in them is a fact about this tree.\n"
                f"    Deliberately keeping them: NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS=1, and\n"
                f"    say why. It silences THIS subject only — a mixed manifest link or a\n"
                f"    crossed venv interpreter still fails, because those are cheap to fix."
            )

    if not manifest_only:
        crossed_scripts: list[tuple[str, str, str]] = []
        for script, interp in venv_shebangs(here, ws):
            subjects += 1
            owner = foreign_owner(interp, here, marker)
            if owner:
                crossed_scripts.append((script, interp, owner))
        if crossed_scripts:
            # ONE finding, not one per script: a crossed venv crosses every
            # console script in it, and 13 identical paragraphs bury the remedy.
            venvs = sorted({os.path.dirname(os.path.dirname(sc)) for sc, _, _ in crossed_scripts})
            names = ", ".join(sorted(os.path.basename(sc) for sc, _, _ in crossed_scripts))
            problems.append(
                f"{len(crossed_scripts)} venv console script(s) run ANOTHER checkout's "
                f"interpreter\n"
                f"      {venvs[0]}/bin/{{{names}}}\n"
                f"      #!{crossed_scripts[0][1]}\n"
                f"      owned by {crossed_scripts[0][2]}\n"
                f"    `west` started this way puts that interpreter in WEST_PYTHON and\n"
                f"    _Python3_EXECUTABLE in every Zephyr CMakeCache.txt — which is how the\n"
                f"    crossing outlives a manifest repair. The spelling is the evidence: the\n"
                f"    interpreter itself is a symlink to the system python, so a check that\n"
                f"    only resolved it would report nothing. Recreate the venv here:\n"
                + "".join(f"      rm -rf {v} && just zephyr setup\n" for v in venvs).rstrip()
            )

    return problems, subjects


# --------------------------------------------------------------------------
# The repair
# --------------------------------------------------------------------------


def foreign_build_dirs(ws: str, here: str, marker: str) -> list[tuple[str, str, str]]:
    """`(build_dir, condemning NAME, its VALUE)` for each dir built from two trees.

    Reuses `foreign_owner` rather than restating the rule: a second spelling of
    "which checkout owns this path" is the defect this file exists to catch
    (issue 1280's gate refuses a fourth one).

    One finding per DIRECTORY, not per entry — the measured case is ~70 caches
    x ~10 crossed variables, and the unit of repair is the directory.
    """
    out: list[tuple[str, str, str]] = []
    for cache in sorted(Path(ws).glob("build*/CMakeCache.txt")):
        for name, value in cache_values(cache):
            owner = foreign_owner(value, here, marker)
            if owner:
                out.append((str(cache.parent), name, value))
                break
    return out


def retire_foreign_build_dirs(ws: str, here: str, marker: str, dry_run: bool) -> int:
    """Remove the build dirs that were configured against ANOTHER checkout.

    This is the one remedy issue 1387 leaves. The module root is a
    CONFIGURE-time identity, so `cmake <build-dir>` cannot correct it and no
    amount of rebuilding inside the directory can either; a pristine build is
    the fix, and removing the directory is how you get one. The scan's own
    message says so: "issue 1387's remedy is a pristine build of the affected
    dirs -- not `cmake <build-dir>`".

    It is NOT the `rm -rf` antipattern CLAUDE.md forbids. That rule is about an
    incremental build producing a WRONG artifact, where the wrongness is a
    missing dependency EDGE and wiping destroys the one reproduction you had.
    Here the directory records a decision taken at configure time against a
    tree that is not this one. There is no edge to find, and the evidence is
    not destroyed: it is printed, per directory, before anything is removed.

    Refuses under `NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS=1` -- that hatch means
    somebody decided to keep them, and a repair that overrode a stated decision
    would be worse than the condition it fixes.
    """
    if os.environ.get("NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS") == "1":
        print(
            "retire-foreign-build-dirs: NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS=1 is set, so "
            "these are kept deliberately; not touching them."
        )
        return 0

    found = foreign_build_dirs(ws, here, marker)
    if not found:
        print(f"retire-foreign-build-dirs: none — no build dir under {ws} names another checkout")
        return 0

    ws_real = _real(ws)
    removed = 0
    for bdir, name, value in found:
        # Three guards, because this deletes: inside the workspace we resolved,
        # a real directory rather than a link, and actually a build dir.
        if not _real(bdir).startswith(ws_real + os.sep):
            print(f"retire-foreign-build-dirs: REFUSING {bdir} — outside {ws_real}", file=sys.stderr)
            return 1
        if os.path.islink(bdir):
            print(f"retire-foreign-build-dirs: REFUSING {bdir} — it is a symlink", file=sys.stderr)
            return 1
        if not os.path.isfile(os.path.join(bdir, "CMakeCache.txt")):
            print(f"retire-foreign-build-dirs: REFUSING {bdir} — no CMakeCache.txt", file=sys.stderr)
            return 1
        print(f"retire-foreign-build-dirs: {'would remove' if dry_run else 'removing'} {bdir}")
        print(f"    condemned by {name}={value}")
        if not dry_run:
            shutil.rmtree(bdir)
        removed += 1

    verb = "would be retired" if dry_run else "retired"
    print(
        f"retire-foreign-build-dirs: {removed} build dir(s) {verb}. They were configured\n"
        f"  against another checkout, so nothing measured in them was a fact about this\n"
        f"  tree; the next build reconfigures them from scratch."
    )
    return 0


# --------------------------------------------------------------------------
# Negative controls, on the normal path
# --------------------------------------------------------------------------


def _make_checkout(path: Path, marker: str) -> str:
    (path / marker).parent.mkdir(parents=True, exist_ok=True)
    (path / marker).write_text("")
    (path / "zephyr").mkdir(parents=True, exist_ok=True)
    (path / "zephyr" / "module.yml").write_text("")
    return str(path)


def _make_workspace(here: Path, name: str = "nano-ros") -> str:
    ws = here / "zephyr-workspace"
    (ws / ".west").mkdir(parents=True, exist_ok=True)
    (ws / "zephyr").mkdir(parents=True, exist_ok=True)
    (ws / ".west" / "config").write_text(f"[manifest]\npath = {name}\nfile = west.yml\n\n[zephyr]\nbase = zephyr\n")
    return str(ws)


def self_test(verbose: bool = False) -> bool:
    """Plant each crossing and prove it is REPORTED; plant the legitimate
    shape and prove it is not. Runs on the normal path, every invocation — a
    negative control nobody runs decays into a comment (issue 1280's gate).
    """
    ok = True
    marker = checkout_marker()

    def chk(control: str, cond: bool, detail: str = "") -> None:
        """`control` states what MUST hold, positively — it is printed either
        way, so a reader of a green run sees the controls that ran rather than
        a list of negated failure texts."""
        nonlocal ok
        if not cond:
            suffix = f" — {detail}" if detail else ""
            print(f"self-test FAILED: {control}{suffix}", file=sys.stderr)
            ok = False
        elif verbose:
            print(f"  ok  {control}")

    with tempfile.TemporaryDirectory(prefix="nros-1387-selftest-") as td:
        tmp = Path(td)
        foreign = _make_checkout(tmp / "other-checkout", marker)
        here = _make_checkout(tmp / "here", marker)
        ws = _make_workspace(tmp / "here")
        proj = os.path.join(ws, "nano-ros")

        # Row 3 of the rule: the symlink that was actually measured.
        os.symlink(foreign, proj)
        problems, subjects = scan(here, ws, marker, manifest_only=False)
        chk(
            "a manifest symlink into a FOREIGN checkout fails the gate",
            len(problems) == 1,
            f"expected exactly one finding, got {problems}",
        )
        chk(
            "the finding names the foreign checkout",
            bool(problems) and foreign in problems[0],
            f"{foreign} absent from {problems}",
        )
        chk(
            "the scanner is non-vacuous (a scan that examines nothing reports nothing)",
            subjects >= 1,
            "zero subjects examined over a real workspace",
        )

        # Row 2: the legitimate self-pointing link that `west init -l` leaves.
        os.remove(proj)
        os.symlink(here, proj)
        problems, _ = scan(here, ws, marker, manifest_only=False)
        chk(
            "the legitimate SELF-pointing manifest link passes",
            not problems,
            f"reported {problems}",
        )

        # A plain directory holding only the manifest (issue 1258's shape).
        os.remove(proj)
        os.makedirs(proj)
        Path(proj, "west.yml").write_text("manifest:\n")
        problems, _ = scan(here, ws, marker, manifest_only=False)
        chk(
            "an unbound manifest project (issue 1258's shape) passes",
            not problems,
            f"reported {problems}",
        )

        # Subject 2: a cached value crossing, beside two that must NOT fire.
        build = Path(ws, "build-c-talker-zenoh")
        build.mkdir()
        outside = tmp / "opt/zephyr-sdk/bin/gcc"
        outside.parent.mkdir(parents=True)
        outside.write_text("")
        Path(build, "CMakeCache.txt").write_text(
            f"_NROS_RMW_DISPATCH_DIR:PATH={foreign}/packages/rmw/dispatch\n"
            f"APPLICATION_SOURCE_DIR:PATH={here}/examples/zephyr/rust/talker\n"
            f"CMAKE_C_COMPILER:FILEPATH={outside}\n"
            f"SOME_FLAGS:STRING=-Wall -Wextra\n"
        )
        problems, _ = scan(here, ws, marker, manifest_only=False)
        chk(
            "a cached value naming a foreign checkout fails the gate",
            len(problems) == 1,
            f"expected exactly one finding, got {problems}",
        )
        chk(
            "an out-of-tree SDK path and this tree's own path do NOT count",
            bool(problems) and problems[0].startswith("1 cached value(s)"),
            f"counted more than the one crossed value: {problems}",
        )
        chk(
            "--manifest-only stops at subject 1",
            not scan(here, ws, marker, manifest_only=True)[0],
            "the manifest-only scan reached the build caches",
        )

        # The escape hatch silences subject 2 and only subject 2.
        os.environ["NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS"] = "1"
        hatched, _ = scan(here, ws, marker, manifest_only=False)
        os.environ.pop("NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS")
        chk(
            "NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS silences the cache subject",
            not hatched,
            f"still reported {hatched}",
        )

        # Subject 3: the venv shebang, which survives a manifest repair.
        binp = Path(here, "scripts/zephyr/.venv/bin")
        binp.mkdir(parents=True)
        # The measured shape: a venv interpreter is a SYMLINK to the system
        # python, so the crossing lives in the spelling and nowhere else.
        foreign_venv = Path(foreign, "scripts/zephyr/.venv/bin")
        foreign_venv.mkdir(parents=True)
        system_py = tmp / "usr/bin/python3"
        system_py.parent.mkdir(parents=True)
        system_py.write_text("")
        os.symlink(system_py, foreign_venv / "python3")
        Path(binp, "west").write_text(f"#!{foreign}/scripts/zephyr/.venv/bin/python3\n")
        Path(binp, "ours").write_text(f"#!{here}/scripts/zephyr/.venv/bin/python3\n")
        Path(binp, "system").write_text("#!/usr/bin/env python3\n")
        problems, _ = scan(here, ws, marker, manifest_only=False)
        chk(
            "a venv shebang naming a foreign interpreter fails the gate, and "
            "ours and the system one do not",
            len(problems) == 2,
            f"expected the cache finding plus one shebang finding, got {problems}",
        )
        os.environ["NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS"] = "1"
        hatched, _ = scan(here, ws, marker, manifest_only=False)
        os.environ.pop("NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS")
        chk(
            "the hatch does NOT silence the venv shebang (configuration, not artifacts)",
            len(hatched) == 1 and "venv console script" in hatched[0],
            f"got {hatched}",
        )

        # Nothing provisioned at all is a SKIP, not a pass.
        bare = _make_checkout(tmp / "bare", marker)
        _, subjects = scan(bare, None, marker, manifest_only=False)
        chk(
            "a checkout with nothing provisioned examines zero subjects (so the "
            "caller can SKIP rather than pass)",
            subjects == 0,
            f"claimed {subjects} subject(s) over a bare checkout",
        )

        # The marker walk itself, both directions.
        chk(
            "the marker walk claims NO owner for a path outside any checkout",
            checkout_root(str(tmp / "opt"), marker) is None,
            "it claimed one",
        )
        chk(
            "the marker walk finds the owner of a path inside one",
            checkout_root(f"{foreign}/packages/x", marker) == foreign,
            "it found none",
        )

    # The REPAIR, both directions. A repair nobody has seen act is a comment,
    # and one that cannot be seen to leave a clean directory alone is worse
    # than the condition — it would delete a tree's own build output.
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        here = _make_checkout(tmp / "here", marker)
        foreign = _make_checkout(tmp / "other", marker)
        ws = _make_workspace(tmp / "here")

        crossed = Path(ws) / "build-crossed"
        crossed.mkdir(parents=True, exist_ok=True)
        (crossed / "CMakeCache.txt").write_text(
            f"NROS_REPO_DIR:PATH={foreign}\n_NROS_MESSAGE_BOUNDS_DIR:PATH={foreign}/cmake\n"
        )
        clean = Path(ws) / "build-clean"
        clean.mkdir(parents=True, exist_ok=True)
        (clean / "CMakeCache.txt").write_text(
            f"NROS_REPO_DIR:PATH={here}\nCMAKE_MAKE_PROGRAM:FILEPATH=/usr/bin/ninja\n"
        )

        chk(
            "the repair names exactly the build dir configured against another checkout",
            [d for d, _, _ in foreign_build_dirs(ws, here, marker)] == [str(crossed)],
            f"it named {[d for d, _, _ in foreign_build_dirs(ws, here, marker)]}",
        )

        rc = retire_foreign_build_dirs(ws, here, marker, dry_run=True)
        chk(
            "a dry run removes nothing",
            rc == 0 and crossed.is_dir() and clean.is_dir(),
            "it removed something",
        )

        rc = retire_foreign_build_dirs(ws, here, marker, dry_run=False)
        chk(
            "the repair removes the crossed dir and leaves the clean one",
            rc == 0 and not crossed.exists() and clean.is_dir(),
            f"crossed={crossed.exists()} clean={clean.is_dir()}",
        )

        os.environ["NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS"] = "1"
        again = Path(ws) / "build-crossed2"
        again.mkdir(parents=True, exist_ok=True)
        (again / "CMakeCache.txt").write_text(f"NROS_REPO_DIR:PATH={foreign}\n")
        rc = retire_foreign_build_dirs(ws, here, marker, dry_run=False)
        chk(
            "the opt-out hatch stops the repair acting on a stated decision",
            rc == 0 and again.is_dir(),
            "it deleted a directory somebody chose to keep",
        )
        del os.environ["NROS_ALLOW_FOREIGN_BUILD_ARTIFACTS"]

    if ok and not verbose:
        # STDERR: the skip path's stdout is the `nros_check_skip` REASON, and a
        # reason carrying a newline writes a second, bogus ledger row.
        print(
            "  self-test ok: foreign manifest link reported, self-pointing link and "
            "unbound project clean, cache crossing reported beside an out-of-tree "
            "and an in-tree value, venv shebang reported, bare checkout examines nothing, "
            "repair retires the crossed dir and spares the clean one",
            file=sys.stderr,
        )
    return ok


# --------------------------------------------------------------------------


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--workspace", help="the workspace to examine (default: the resolver's)")
    ap.add_argument(
        "--manifest-only",
        action="store_true",
        help="subject 1 only — the shape `check-zephyr-workspace-checkout.sh` front-runs",
    )
    ap.add_argument("--self-test", action="store_true", help="the negative controls, verbosely")
    ap.add_argument(
        "--retire-foreign-build-dirs",
        action="store_true",
        help="REMOVE the build dirs configured against another checkout (issue 1387's remedy)",
    )
    ap.add_argument(
        "--dry-run",
        action="store_true",
        help="with --retire-foreign-build-dirs: name them without removing anything",
    )
    args = ap.parse_args()

    marker = checkout_marker()

    if args.self_test:
        return 0 if self_test(verbose=True) else 1

    if not self_test():
        print("check-zephyr-workspace-foreign-checkout: SELF-TEST FAILED", file=sys.stderr)
        return 1

    here = str(ROOT)
    ws = args.workspace or resolve_workspace(here)
    if ws and not os.path.isdir(ws):
        ws = None

    if args.retire_foreign_build_dirs:
        if not ws:
            print("retire-foreign-build-dirs: no Zephyr workspace resolves here; nothing to do")
            return 0
        return retire_foreign_build_dirs(ws, here, marker, args.dry_run)

    problems, subjects = scan(here, ws, marker, args.manifest_only)

    if subjects == 0:
        where = "no Zephyr workspace resolves here" if not ws else f"{ws} holds none of its subjects"
        print(
            f"nothing provisioned to examine ({where}); "
            "`just zephyr setup` provisions one — issue 1387"
        )
        return EXIT_SKIP

    if problems:
        print("check-zephyr-workspace-foreign-checkout: FAILED (issue 1387)", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        print(
            "\nOne image built from two checkouts BUILDS, links and runs; the halves\n"
            "disagree only when one of them changes, so nothing else will tell you.",
            file=sys.stderr,
        )
        return 1

    scope = "manifest project" if args.manifest_only else "manifest project, build caches, venv shebangs"
    print(
        f"check-zephyr-workspace-foreign-checkout: ok — {subjects} subject(s) examined "
        f"({scope}) in {ws}, none names another checkout"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
