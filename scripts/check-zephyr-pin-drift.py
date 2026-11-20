#!/usr/bin/env python3
"""Issue 2697 -- a provisioned Zephyr workspace whose projects sit off the pin.

`west.yml` pins `zephyr` at `revision: v3.7.2` (issue 2389), and a workspace
provisioned before that move stayed at v3.7.0 for six days on one host: every
local 3.7 build there used a kernel CI does not, and only the boot banner said
so. `nros doctor` and `_require-owned-provisioned-roots` ask WHOSE a workspace
is (issues 1387/1395), never WHICH REVISION it holds, and a west build against
the wrong tree succeeds.

This asks it. For each Zephyr line with a provisioned workspace
(`scripts/lib/zephyr-workspace.sh --version <line> resolve`; a line nobody
provisioned is not drift), it reads the manifest the workspace was provisioned
FROM -- the file `.west/config` names, taken from THIS CHECKOUT, never the
workspace's own copy, which goes stale with the tree -- and classifies every
project's HEAD against its pinned revision:

  * the manifest's own projects (`zephyr`, `zephyr-lang-rust`, ...);
  * the projects `zephyr` IMPORTS (its `name-allowlist`: cmsis, picolibc, ...),
    whose pins live in Zephyr's own `west.yml` AT THE PINNED ZEPHYR COMMIT. A
    kernel move moves them, so checking only the top level would miss half of
    what a pin change changes.

The classification is the ONE classifier (`scripts/lib/pin-drift.sh`, issue
2701); the policy is this caller's:

  behind / diverged / unfetched  FAIL: the build would use a tree the pin does
                                 not name. Remedy: re-sync the workspace's
                                 manifest from this checkout, then
                                 `west update` that project.
  ahead                          WARN: local commits on a pinned Zephyr are no
                                 shape of ours, but they are somebody's work.
  shallow                        NOT VERIFIED (a cut history measures nothing).

It compares HEAD only, so the lane's patch scripts (`scripts/zephyr/patches/`),
which edit the work tree, are not drift.

It also asks whether the workspace CARRIES the line's patch set (issue 2764):
a workspace that never got it, or lost it to a `west update`, otherwise reads
as fine until a configure fails. The patch set records each run in
`<ws>/.west/nros-patchset` (`scripts/lib/zephyr-patchset.sh`, the one reader
and writer):

  recorded for this checkout's set   ok
  recorded, but another set's         FAIL: a patch this checkout carries is
                                      missing (or the record names another line)
  no record, no project edited        FAIL: nothing of the patch set is there
  no record, projects edited          NOTE: applied before records existed;
                                      re-running the set records it It never runs `west update` itself: a
store workspace is SHARED by every session on the host, and moving it under a
running build changes that build part-way through.

The self-test runs first on every run, on scratch repositories.

Usage: check-zephyr-pin-drift.py [--line <3.7|4.4>]...   (default: every line)
Exit: 0 clean (notes allowed), 1 drift, 2 usage or self-test failure.
"""

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from lib.git_history import HistoryUnknown, history_git, history_objects_unreadable  # noqa: E402
from lib.git_hook_env import nros_clear_inherited_git_env  # noqa: E402

LINES = ("3.7", "4.4")


def git(repo, *args, check=False):
    p = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)
    if check and p.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)}: {p.stderr.strip()}")
    return p.stdout.strip() if p.returncode == 0 else None


def classify(repo, pinned_commit):
    """The ONE classifier, `nros_pin_drift` (scripts/lib/pin-drift.sh)."""
    p = subprocess.run(
        ["bash", "-c", '. "$1" && nros_pin_drift "$2" "$3"', "_",
         str(ROOT / "scripts/lib/pin-drift.sh"), str(repo), pinned_commit],
        capture_output=True, text=True)
    return p.stdout.strip() or f"not-verified classifier-error {p.stderr.strip()[:80]}"


def resolve(repo, rev):
    """The pinned revision as a commit in `repo`, or the revision itself when
    the repository does not have it (the classifier then says `unfetched`)."""
    for cand in (f"{rev}^{{commit}}", f"refs/tags/{rev}^{{commit}}"):
        c = git(repo, "rev-parse", "--verify", "-q", cand)
        if c:
            return c
    return rev


def manifest_file(ws):
    """The manifest file the workspace was provisioned from (`.west/config`)."""
    cfg = Path(ws) / ".west" / "config"
    try:
        text = cfg.read_text()
    except OSError:
        return None
    section, name = None, None
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("["):
            section = s
            continue
        if section == "[manifest]" and re.match(r"file\s*=", s):
            name = s.split("=", 1)[1].strip()
    return name or "west.yml"


def projects(manifest_text, import_lookup):
    """[(name, path, revision)] for a manifest's projects, plus the projects a
    project IMPORTS through a `name-allowlist`, read by `import_lookup(name,
    revision)` -> that project's own manifest text at the pinned revision."""
    import yaml
    m = (yaml.safe_load(manifest_text) or {}).get("manifest", {})
    default_rev = (m.get("defaults") or {}).get("revision", "master")
    out = []
    for p in m.get("projects", []) or []:
        name = p["name"]
        rev = str(p.get("revision", default_rev))
        out.append((name, p.get("path", name), rev))
        imp = p.get("import")
        allow = imp.get("name-allowlist") if isinstance(imp, dict) else None
        if not allow:
            continue
        sub_text = import_lookup(name, p.get("path", name), rev)
        if sub_text is None:
            out.append((f"{name}'s imports", None, None))
            continue
        sub = (yaml.safe_load(sub_text) or {}).get("manifest", {})
        sub_default = (sub.get("defaults") or {}).get("revision", "master")
        for q in sub.get("projects", []) or []:
            if q["name"] in allow:
                out.append((q["name"], q.get("path", q["name"]), str(q.get("revision", sub_default))))
    return out


def check_workspace(line, ws, checkout):
    """(failures, notes, projects checked) for one provisioned workspace."""
    fails, notes, edited = [], [], []
    mf = manifest_file(ws)
    if mf is None:
        return [f"[x] {line}: {ws}/.west/config is unreadable, so which manifest it follows is unknown"], notes, 0
    want = Path(checkout) / mf
    if not want.is_file():
        return [f"[x] {line}: the workspace was provisioned from `{mf}`, which this checkout does not have"], notes, 0

    def lookup(_name, path, rev):
        # One path at one commit, through the bounded history helper: an
        # unreadable clone or a read that times out is NOT VERIFIED (None).
        repo = Path(ws) / path
        if history_objects_unreadable(str(repo)):
            return None
        commit = resolve(repo, rev)
        try:
            r = history_git(str(repo), "show", f"{commit}:west.yml", check=False, fetch=True)
        except HistoryUnknown:
            return None
        return r.stdout if r.returncode == 0 else None

    remedy = (f"scripts/zephyr/sync-workspace-manifest.sh {ws} {checkout}"
              "   (re-syncs the workspace manifest, then runs west update)")
    checked = 0
    for name, path, rev in projects(want.read_text(), lookup):
        if path is None:
            notes.append(f"NOT VERIFIED: {line}: {name} -- the pinned zephyr commit is not fetched, "
                         "so its imported projects' pins cannot be read")
            continue
        repo = Path(ws) / path
        if not (repo / ".git").exists():
            notes.append(f"note: {line}: {name} ({path}) is not checked out in {ws}")
            continue
        checked += 1
        if subprocess.run(["git", "-C", str(repo), "diff", "--quiet", "HEAD", "--"],
                          capture_output=True).returncode == 1:
            edited.append(name)
        state = classify(repo, resolve(repo, rev))
        head = git(repo, "rev-parse", "--short=9", "HEAD") or "?"
        desc = git(repo, "describe", "--tags", "--always", "HEAD") or head
        if state == "ok":
            continue
        if state.startswith("ahead"):
            notes.append(f"WARN: {line}: {name} ({path}) is {state.split()[1]} commit(s) AHEAD of its pin "
                         f"`{rev}` (HEAD {desc}). Local commits on a pinned Zephyr project are not "
                         f"a shape of ours; to return to the pin: {remedy}")
        elif state == "not-verified shallow":
            notes.append(f"NOT VERIFIED: {line}: {name} ({path}) has a cut history, so its "
                         f"place against `{rev}` is not a measurement")
        else:
            what = {"not-verified unfetched": f"its pin `{rev}` is not fetched there",
                    "diverged": f"it has DIVERGED from its pin `{rev}`"}.get(
                state, f"it is {state.split()[-1]} commit(s) BEHIND its pin `{rev}`"
                if state.startswith("behind") else f"`{state}` against its pin `{rev}`")
            fails.append(f"[x] {line}: {name} ({ws}/{path}) is at {desc}; {what}\n"
                         f"      remedy: {remedy}")
    if checked:
        f, n = patchset_findings(line, ws, checkout, edited)
        fails += f
        notes += n
    return fails, notes, checked


def patchset_state(ws, checkout):
    """`nros_zephyr_patchset_state` (scripts/lib/zephyr-patchset.sh), the one
    reader of the record, as its words."""
    p = subprocess.run(
        ["bash", "-c", '. "$1" && nros_zephyr_patchset_state "$2" "$3"', "_",
         str(ROOT / "scripts/lib/zephyr-patchset.sh"), str(ws), str(checkout)],
        capture_output=True, text=True)
    return (p.stdout.strip() or f"error {p.stderr.strip()[:80]}").split()


def patchset_findings(line, ws, checkout, edited):
    """(failures, notes) for whether `ws` carries this checkout's patch set.
    `edited` names the pinned projects whose work tree differs from HEAD."""
    state = patchset_state(ws, checkout)
    kind, zl = state[0], (state[1] if len(state) > 1 else line)
    remedy = f"bash scripts/zephyr/patches/{zl}.sh {ws}   (idempotent; records that it ran)"
    if kind == "ok":
        return [], []
    if kind == "unknown-line":
        return [], [f"NOT VERIFIED: {line}: {ws}/zephyr/VERSION is unreadable, so which patch set "
                    "the workspace needs is unknown"]
    if kind == "no-patchset":
        return [f"[x] {line}: {ws} checks out Zephyr {zl}, and this checkout has no "
                f"scripts/zephyr/patches/{zl}.sh for it\n"
                "      remedy: provision the workspace for a line this checkout supports"], []
    if kind == "wrong-line":
        return [f"[x] {line}: {ws} checks out Zephyr {state[2]}, but its patch record names "
                f"{state[1]}\n      remedy: bash scripts/zephyr/patches/{state[2]}.sh {ws}"], []
    if kind == "stale":
        return [f"[x] {line}: {ws} has a patch record, but not for this checkout's {zl} patch set, "
                f"so a patch it carries may be missing there\n      remedy: {remedy}"], []
    if kind == "unrecorded" and not edited:
        return [f"[x] {line}: {ws} carries NONE of nano-ros's {zl} patch set (no record, and no "
                f"pinned project has a local edit); a build there fails at configure\n"
                f"      remedy: {remedy}"], []
    if kind == "unrecorded":
        return [], [f"note: {line}: {ws} has local edits in {', '.join(edited)} but no patch record "
                    f"(applied before records existed?); to record it: {remedy}"]
    return [], [f"NOT VERIFIED: {line}: could not read the patch record ({' '.join(state)})"]


def workspace_for(line):
    p = subprocess.run(["bash", str(ROOT / "scripts/lib/zephyr-workspace.sh"),
                        "--version", line, "resolve"], capture_output=True, text=True)
    ws = p.stdout.strip()
    return ws if p.returncode == 0 and ws and (Path(ws) / ".west").is_dir() else None


def self_test():
    """Scratch workspace: a pinned project at its tag (ok), one tag behind
    (FAIL), an unfetched pin (FAIL), one local commit ahead (WARN), a
    work-tree edit at the pin (ok), and an import read at the pinned commit."""
    bad = []
    env = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t", GIT_COMMITTER_NAME="t",
               GIT_COMMITTER_EMAIL="t@t", GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_NOSYSTEM="1")
    for k in [k for k in env if k.startswith("GIT_") and k not in (
            "GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL", "GIT_COMMITTER_NAME", "GIT_COMMITTER_EMAIL",
            "GIT_CONFIG_GLOBAL", "GIT_CONFIG_NOSYSTEM")]:
        env.pop(k)

    def g(repo, *a):
        subprocess.run(["git", "-C", str(repo), *a], env=env, check=True, capture_output=True)

    def repo_with(path, commits, tag_at=None):
        path.mkdir(parents=True)
        g(path, "init", "-q")
        for i, files in enumerate(commits):
            for f, body in files.items():
                (path / f).write_text(body)
            g(path, "add", ".")
            g(path, "commit", "-q", "-m", f"c{i}")
            if tag_at == i:
                g(path, "tag", "v1")
        return path

    with tempfile.TemporaryDirectory(prefix="zephyr-pin-drift-") as t:
        t = Path(t)
        ws, co = t / "ws", t / "co"
        (ws / ".west").mkdir(parents=True)
        (ws / ".west" / "config").write_text("[manifest]\npath = nros\nfile = west.yml\n")
        zephyr_manifest = "manifest:\n  projects:\n    - name: cmsis\n      path: modules/cmsis\n      revision: v1\n    - name: other\n      revision: v1\n"
        repo_with(ws / "zephyr", [{"west.yml": zephyr_manifest}, {"k": "x"}], tag_at=0)
        # zephyr: HEAD is ONE AHEAD of the pin v1 -> WARN; its import list read at v1.
        repo_with(ws / "modules" / "cmsis", [{"a": "1"}, {"a": "2"}], tag_at=1)
        g(ws / "modules" / "cmsis", "checkout", "-q", "HEAD~1")          # cmsis BEHIND v1 -> FAIL
        rust = repo_with(ws / "modules" / "lang" / "rust", [{"r": "1"}], tag_at=0)
        (rust / "r").write_text("patched")                                 # work-tree edit -> ok
        repo_with(ws / "unf", [{"u": "1"}])                                # pin never fetched -> FAIL
        co.mkdir()
        (co / "west.yml").write_text(
            "manifest:\n  projects:\n"
            "    - name: zephyr\n      revision: v1\n      import:\n        name-allowlist: [cmsis]\n"
            "    - name: zephyr-lang-rust\n      path: modules/lang/rust\n      revision: v1\n"
            "    - name: unf\n      revision: 0123456789abcdef0123456789abcdef01234567\n")
        fails, notes, checked = check_workspace("3.7", str(ws), str(co))
        text = "\n".join(fails + notes)
        cases = [
            ("a project behind its tag is a FAIL", any("cmsis" in f and "BEHIND" in f for f in fails)),
            ("an unfetched pin is a FAIL", any("unf" in f and "not fetched" in f for f in fails)),
            ("a project ahead of its pin is a WARN, not a FAIL",
             any("zephyr (zephyr)" in n and "AHEAD" in n for n in notes)
             and not any(f.startswith("[x] 3.7: zephyr (") for f in fails)),
            ("a work-tree edit at the pin is not drift", "zephyr-lang-rust" not in text),
            ("an import outside the allowlist is not checked", "other" not in text),
            ("every failure names its remedy", bool(fails) and all("remedy:" in f for f in fails)),
            ("every checked-out pinned project is counted", checked == 4),
        ]
        for name, ok in cases:
            if not ok:
                bad.append(name)
        if bad:
            print("check-zephyr-pin-drift self-test: FAILED\n  " + "\n  ".join(bad)
                  + "\n--- what it reported:\n" + text, file=sys.stderr)
            return False

    # The patch record (issue 2764): written by the real recorder, read by the
    # real reader, against a scratch checkout's patch set.
    with tempfile.TemporaryDirectory(prefix="zephyr-patchset-") as t:
        t = Path(t)
        ws, co = t / "ws", t / "co"
        (ws / ".west").mkdir(parents=True)
        (ws / "zephyr").mkdir()
        (ws / "zephyr" / "VERSION").write_text("VERSION_MAJOR = 3\nVERSION_MINOR = 7\nPATCHLEVEL = 2\n")
        (co / "scripts" / "zephyr" / "patches").mkdir(parents=True)
        (co / "scripts" / "zephyr" / "one-patch.sh").write_text("echo one\n")
        (co / "scripts" / "zephyr" / "patches" / "3.7.sh").write_text(
            "bash ./scripts/zephyr/one-patch.sh \"$WORKSPACE\"\n")

        def record():
            subprocess.run(["bash", "-c", '. "$1" && nros_zephyr_patchset_record "$2" "$3" 3.7', "_",
                            str(ROOT / "scripts/lib/zephyr-patchset.sh"), str(ws), str(co)],
                           check=True, capture_output=True)

        def joined(edited):
            f, n = patchset_findings("3.7", str(ws), str(co), edited)
            return f, n, "\n".join(f + n)

        f0, n0, _ = joined([])
        f1, n1, _ = joined(["zephyr"])
        record()
        f2, n2, _ = joined([])
        (co / "scripts" / "zephyr" / "one-patch.sh").write_text("echo one, grown\n")
        f3, _, t3 = joined(["zephyr"])
        record()
        f4, n4, _ = joined([])
        (co / "scripts" / "zephyr" / "one-patch.sh").write_text("echo one\n")
        f5, n5, _ = joined([])
        (ws / "zephyr" / "VERSION").write_text("VERSION_MAJOR = 4\nVERSION_MINOR = 4\n")
        f6, _, t6 = joined(["zephyr"])
        cases = [
            ("no record and no edited project is a FAIL", len(f0) == 1 and "NONE" in f0[0]),
            ("no record over edited projects is a NOTE, not a FAIL", not f1 and len(n1) == 1),
            ("a recorded set is ok", not f2 and not n2),
            ("a set whose patch changed since it was recorded is a FAIL, even over edits",
             len(f3) == 1 and "not for this checkout" in t3),
            ("re-running the grown set clears it", not f4 and not n4),
            ("the older set, applied before, is still recorded", not f5 and not n5),
            ("a record for another line than the tree is a FAIL", len(f6) == 1 and "4.4" in t6),
            ("every patch-record failure names its remedy",
             all("remedy:" in f for f in f0 + f3 + f6)),
        ]
        for name, ok in cases:
            if not ok:
                bad.append(name)
        if bad:
            print("check-zephyr-pin-drift self-test (patch record): FAILED\n  " + "\n  ".join(bad)
                  + f"\n--- f0={f0} n1={n1} f3={f3} f6={f6}", file=sys.stderr)
            return False
    return True


def main(argv):
    lines = []
    it = iter(argv)
    for a in it:
        if a == "--line":
            lines.append(next(it, ""))
        else:
            print("usage: check-zephyr-pin-drift.py [--line <3.7|4.4>]...", file=sys.stderr)
            return 2
    if any(ln not in LINES for ln in lines):
        print(f"check-zephyr-pin-drift: --line takes one of {', '.join(LINES)}", file=sys.stderr)
        return 2
    if not self_test():
        return 2
    fails, notes, seen, projects_checked = [], [], 0, 0
    for line in lines or LINES:
        ws = workspace_for(line)
        if not ws:
            continue
        seen += 1
        f, n, c = check_workspace(line, ws, str(ROOT))
        fails += f
        notes += n
        projects_checked += c
    for n in notes:
        print(f"check-zephyr-pin-drift: {n}", file=sys.stderr)
    if fails:
        print("check-zephyr-pin-drift: a provisioned Zephyr workspace is off this checkout's pin or "
              "lacks its patch set, so a build there uses a tree this checkout does not describe:",
              file=sys.stderr)
        print("\n".join(f"  {f}" for f in fails), file=sys.stderr)
        print("  The workspace is shared by every session on this host: move it when no build is using it.",
              file=sys.stderr)
        return 1
    print(f"check-zephyr-pin-drift: OK -- {seen} provisioned workspace(s), {projects_checked} "
          "pinned project(s), each at this checkout's pin and carrying its patch set")
    return 0


if __name__ == "__main__":
    # Every git call here reads a WORKSPACE project (or a scratch repository
    # in the self-test), never this checkout. Reached from a git hook, an
    # inherited GIT_DIR would point each of them at the caller's repository
    # instead, so clear it once, before anything runs.
    nros_clear_inherited_git_env()
    sys.exit(main(sys.argv[1:]))
