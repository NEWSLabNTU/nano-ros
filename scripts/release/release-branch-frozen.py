#!/usr/bin/env python3
"""Refuse a release branch that was re-cut after its version published (phase-567 W2).

User ruling 2026-11-16: `release/<ver>` may be re-cut from main under the same
version name for as long as that version is UNPUBLISHED, which is when no
`nros-<ver>` tag exists. Once the tag exists the name is burned: the branch is
frozen at the tagged commit, and anything after it reaches the branch only as
a cherry-pick from main (`git cherry-pick -x`, which writes a `(cherry picked
from commit <sha>)` line). A re-cut after publishing would ship whatever else
landed on main since, past every check the published cut was measured with.

  CUT        `nros-<ver>`'s commit when the tag exists; otherwise the branch's
             merge-base with main (re-cutting moves it, and that is allowed).
  FROZEN     the cut is an ancestor of HEAD, and every commit in cut..HEAD
             carries a `(cherry picked from commit X)` line whose X is on main.
  REFUSED    names the tag and each commit that is not a cherry-pick of a
             main commit, or says the tagged commit is not on the branch.
  MEASURED   the cut and each cherry-pick's source on main are written to
             --measure-out: W1 (`release-lanes-green.py`) measures those,
             because the lanes run on main and never on a release branch.

  release-branch-frozen.py --branch release/<x> --version <ver> [--main origin/main] [--measure-out F]
  release-branch-frozen.py --self-test        (`just check release-branch-frozen`)

Needs the full history and the tags (`fetch-depth: 0`). Every invocation runs
the self-test first, and a release is refused if it fails.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402
from git_history import UNKNOWN, ancestry  # noqa: E402  (a NO from a shallow clone is not a verdict)

PICK = re.compile(r"^\(cherry picked from commit ([0-9a-f]{7,40})\)\s*$", re.M)


def git(args, cwd):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True)


def tag_commit(cwd, version):
    p = git(["rev-parse", "-q", "--verify", f"refs/tags/nros-{version}^{{commit}}"], cwd)
    return p.stdout.strip() if p.returncode == 0 else None


def check(cwd, head, main, version):
    """(ok, [lines], [main commits W1 must measure])."""
    tag = f"nros-{version}"
    cut = tag_commit(cwd, version)
    published = cut is not None
    if not published:
        p = git(["merge-base", head, main], cwd)
        if p.returncode != 0:
            return False, [f"{head[:10]} shares no history with {main}"], []
        cut = p.stdout.strip()
        origin = f"its merge-base with {main}; {tag} does not exist, so a re-cut is allowed"
    else:
        origin = f"{tag}, published"
        a = ancestry(cwd, cut, head)
        if a is UNKNOWN:
            return False, [f"cannot tell whether {tag} ({cut[:10]}) is an ancestor of {head[:10]} "
                           "(truncated history: check out with fetch-depth 0)"], []
        if not a:
            return False, [f"{tag} ({cut[:10]}) is not an ancestor of {head[:10]}: the branch was "
                           f"re-cut or rewound after {version} was published. The name is burned "
                           "once its tag exists: cherry-pick onto the tagged commit, or release "
                           "under a new version"], []
    shas = git(["rev-list", "--reverse", f"{cut}..{head}"], cwd).stdout.split()
    bad, measure = [], [cut]
    for sha in shas:
        msg = git(["show", "-s", "--format=%B", sha], cwd).stdout
        subj = msg.splitlines()[0] if msg else ""
        srcs = PICK.findall(msg)
        if not srcs:
            bad.append(f"  {sha[:10]} {subj}  (no `(cherry picked from commit …)` line)")
            continue
        measure += [git(["rev-parse", s], cwd).stdout.strip() or s for s in srcs]
        # UNKNOWN (truncated history) refuses too: a release is never cut on
        # a "cannot tell".
        off = [s for s in srcs if ancestry(cwd, s, main) is not True]
        if off:
            bad.append(f"  {sha[:10]} {subj}  (picked from {', '.join(x[:10] for x in off)}, not on {main})")
    if bad:
        rule = (f"{version} is published, so its branch is frozen at {tag}: `git cherry-pick -x "
                "<main-sha>` onto it, or release under a new version (phase-567 W2)."
                if published else
                f"{tag} does not exist yet, so the branch may be re-cut from {main}; anything on "
                "top of the cut must still be a `git cherry-pick -x` of a main commit (phase-567 W2).")
        return False, [f"{len(bad)} of {len(shas)} commit(s) since the cut {cut[:10]} ({origin}) "
                       f"are not cherry-picks of a {main} commit:", *bad, rule], []
    return True, [f"frozen: cut {cut[:10]} ({origin}), then {len(shas)} cherry-pick(s) from {main}"], measure


# ------------------------------------------------------------------ self-test

def self_test():
    env = nros_clear_inherited_git_env(dict(os.environ))
    env.update(GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t", GIT_COMMITTER_NAME="t",
               GIT_COMMITTER_EMAIL="t@t", GIT_CONFIG_NOSYSTEM="1")
    saved = dict(os.environ)
    os.environ.clear()
    os.environ.update(env)
    fails = []
    try:
        with tempfile.TemporaryDirectory() as t:
            r = str(Path(t))

            def g(*a):
                p = git(list(a), r)
                if p.returncode != 0:
                    raise RuntimeError(f"git {a}: {p.stderr}")
                return p.stdout.strip()

            n = [0]

            def commit(name):
                n[0] += 1
                Path(r, f"{name}.txt").write_text(str(n[0]))
                g("add", f"{name}.txt")
                g("commit", "-q", "-m", name)
                return g("rev-parse", "HEAD")

            g("init", "-q", "-b", "main")
            m1 = commit("m1")
            cut = commit("m2")                       # the cut
            fix = commit("fix-on-main")              # a fix that lands on main
            m4 = commit("m4")                        # unrelated main work
            g("checkout", "-q", "-b", "side", cut)
            stray = commit("not-on-main")            # never on main
            g("checkout", "-q", "-b", "release", cut)
            g("cherry-pick", "-x", fix)
            picked = g("rev-parse", "HEAD")
            plain = commit("hand-edit")
            g("checkout", "-q", "-b", "rel2", cut)
            g("cherry-pick", "-x", stray)
            off = g("rev-parse", "HEAD")
            g("checkout", "-q", "main")

            def run(head, ver):
                return check(r, head, "main", ver)

            cases = []
            # --- unpublished: no nros-1.0 tag
            ok, out, meas = run(cut, "1.0")
            cases.append(("the cut alone, unpublished", ok, True, out))
            ok, out, meas = run(picked, "1.0")
            cases.append(("the cut + a cherry-pick -x, unpublished", ok, True, out))
            if meas != [cut, fix]:
                fails.append(f"the commits W1 measures: want the cut and the pick's source, got {meas}")
            ok, out, meas = run(m4, "1.0")
            cases.append(("re-cut forward from main, unpublished (allowed)", ok, True, out))
            if meas != [m4]:
                fails.append(f"an unpublished re-cut measures its new cut: want [m4], got {meas}")
            ok, out, meas = run(plain, "1.0")
            cases.append(("a plain commit on top of the cut, unpublished", ok, False, out))
            ok, out, meas = run(off, "1.0")
            cases.append(("a cherry-pick whose source is not on main", ok, False, out))
            if not ok and "not on main" not in "\n".join(out):
                fails.append(f"off-main pick: the refusal does not say so: {out}")
            # --- published: nros-2.0 tags the cut
            g("tag", "nros-2.0", cut)
            ok, out, meas = run(picked, "2.0")
            cases.append(("published: the tag + a cherry-pick -x", ok, True, out))
            ok, out, meas = run(m4, "2.0")
            cases.append(("published: re-cut forward from main (refused)", ok, False, out))
            if not ok and not (any(fix[:10] in x for x in out) and any(m4[:10] in x for x in out)
                               and "nros-2.0" in "\n".join(out)):
                fails.append(f"published re-cut: the refusal does not name the tag and both commits: {out}")
            ok, out, meas = run(m1, "2.0")
            cases.append(("published: rewound behind the tag (refused)", ok, False, out))
            if not ok and "burned" not in out[0]:
                fails.append(f"published rewind: the refusal does not say the name is burned: {out}")
            for name, got, want, out in cases:
                if got != want:
                    fails.append(f"{name}: want {'frozen' if want else 'refused'}, got "
                                 f"{'frozen' if got else 'refused'}: {out}")
    finally:
        os.environ.clear()
        os.environ.update(saved)
    for f in fails:
        print(f"release-branch-frozen self-test FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("release-branch-frozen self-test: OK (cut, cherry-pick -x, unpublished re-cut allowed, "
          "plain commit, off-main pick, published pick, published re-cut refused, published rewind, "
          "the commits W1 measures)")
    return 0


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--branch")
    ap.add_argument("--version")
    ap.add_argument("--main", default="origin/main")
    ap.add_argument("--measure-out", help="write the main commits W1 must measure, one per line")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args(argv)
    rc = self_test()
    if a.self_test or rc != 0:
        if rc != 0 and not a.self_test:
            print("::error::release-branch-frozen: its self-test fails; the release is refused")
        return rc
    if not a.branch or not a.version:
        ap.error("--branch and --version are required")
    nros_clear_inherited_git_env()
    cwd = os.getcwd()
    head = git(["rev-parse", "HEAD"], cwd).stdout.strip()

    def emit(shas):
        if a.measure_out:
            Path(a.measure_out).write_text("".join(x + "\n" for x in shas))

    if not a.branch.startswith("release/"):
        print(f"release-branch-frozen: {a.branch} is not a release/<ver> branch; nothing to check")
        emit([head])
        return 0
    ok, out, measure = check(cwd, head, a.main, a.version)
    emit(measure)
    print(("" if ok else "::error::") + out[0])
    for line in out[1:]:
        print(line)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
