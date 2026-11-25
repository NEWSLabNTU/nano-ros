#!/usr/bin/env python3
"""Issue 2957 -- a branch that edits what the island-shaped Zephyr image's
configure reads either regenerates the snapshot the merge queue checks, or
states that a live check found it current.

The merge queue checks knob delivery for that image over a committed snapshot
of its configured build dir (tests/fixtures/knob-delivery/island-shaped-zephyr,
phase-567 W4). Only the daily Zephyr lane configures the image live, so a
change to what the configure resolves (issues 2886 and 2780) merged green and
turned the nightly red a day later, inside a release cut.

The snapshot's `inputs.txt` lists the tracked files on the configure's
RERUN_CMAKE edge -- every CMake and Kconfig file whose edit re-runs it. It is
written by the regen from the live build, so it follows the resolver without
anyone keeping a list. This gate reads the branch (merge base with
origin/main to HEAD) and REFUSES it when:

  - the branch changes a listed file, and
  - the branch leaves the snapshot untouched, and
  - no commit on the branch carries `Island-Snapshot: checked <digest>` with
    the digest of the listed files at HEAD.

`just check island-shaped-zephyr` prints that trailer after it configured the
image live and found the snapshot equal. The digest covers the inputs it
configured, so an input edited after the check moves the digest and the
trailer no longer counts.

The census half of the snapshot (the NROS_DERIVED_* counts, issue 2886's
class) needs no listing: `island-shaped-workspace` diffs it against the
census on every pull request.

A merge-queue, push, schedule or dispatch run judges no branch (the events
and the base are changelog-audit.py's, one spelling) and passes.

  check-island-snapshot-inputs.py [--base <ref>]
  check-island-snapshot-inputs.py --self-test
"""
import importlib.util
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402
import check_skip  # noqa: E402  the NOT VERIFIED ledger
import git_history  # noqa: E402

NAME = "island-snapshot-inputs"


def _load(name, rel):
    spec = importlib.util.spec_from_file_location(name, str(ROOT / rel))
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


FIX = _load("knob_delivery_fixture", "scripts/knob-delivery-fixture.py")
AUDIT = _load("changelog_audit", "scripts/changelog-audit.py")


def _git(cwd, *args):
    p = subprocess.run(["git", "-C", str(cwd)] + list(args), capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError("git %s: %s" % (" ".join(args), p.stderr.strip()))
    return p.stdout


def judge(cwd, base, head="HEAD"):
    """(ok, message) for the branch base..head in the repository at cwd."""
    fixture = FIX.FIXTURE
    # One path at the tip, and one two-tree diff: a handful of objects, so a
    # treeless CI clone may fetch them lazily, bounded (git_history's
    # `fetch=True` case, changelog-audit.py's read). A timeout raises
    # HistoryUnknown, which the caller reports NOT VERIFIED.
    lazy = not all(git_history.tree_present(str(cwd), rev) for rev in (base, head))
    shown = git_history.history_git(str(cwd), "show", "%s:%s/%s" % (head, fixture, FIX.INPUTS),
                                    check=False, fetch=True)
    text = shown.stdout if shown.returncode == 0 else None
    if text is None:
        return False, ("%s: %s/%s is missing at %s, so this gate cannot tell which files the "
                       "island-shaped Zephyr configure reads. Regenerate the snapshot:\n    %s"
                       % (NAME, fixture, FIX.INPUTS, head, FIX.REGEN))
    inputs = FIX.fixture_inputs(text)
    changed = [p for p in git_history.history_git(
        str(cwd), "diff", "--name-only", "--no-renames", base, head, fetch=lazy).stdout.splitlines() if p]
    hit = sorted(set(changed) & set(inputs))
    if not hit:
        return True, "%s: OK (the branch edits none of the %d configure inputs)" % (NAME, len(inputs))
    if any(p.startswith(fixture + "/") for p in changed):
        return True, "%s: OK (the branch regenerates the snapshot)" % NAME
    want = FIX.digest(str(cwd), inputs, head)
    trailers = _git(cwd, "log", "--format=%(trailers:key=" + FIX.TRAILER + ",valueonly)",
                    "%s..%s" % (base, head)).split()
    stated = [trailers[i + 1] for i, t in enumerate(trailers[:-1]) if t == "checked"]
    if want in stated:
        return True, "%s: OK (checked live at input digest %s)" % (NAME, want)
    lines = ["%s: this branch edits what the island-shaped Zephyr image's configure reads," % NAME,
             "and leaves the snapshot the merge queue checks (%s) as it was:" % fixture]
    lines += ["    %s" % p for p in hit]
    if stated:
        lines.append("A `%s: checked` trailer is present for digest %s, but the inputs at %s "
                     "hash to %s: an input moved after that check." % (
                         FIX.TRAILER, ", ".join(stated), head, want))
    lines += [
        "",
        "On a host with a Zephyr 3.7 west workspace (just setup zephyr), run",
        "    just check island-shaped-zephyr",
        "If it reports the snapshot stale, run `just check island-shaped-zephyr-regen` and",
        "commit the snapshot. If it passes, it prints a trailer; add it to a commit on",
        "this branch (git commit --fixup=reword:<sha>):",
        "    %s: checked %s" % (FIX.TRAILER, want),
        "",
        "Why (issue 2957): the merge queue has no west workspace, so a stale snapshot",
        "merges green and the next nightly turns red.",
    ]
    return False, "\n".join(lines)


def require(argv):
    event = AUDIT.run_event()
    if event in AUDIT.NO_PR_EVENTS:
        print("%s: OK (%s: %s)" % (NAME, event, AUDIT.NO_PR_EVENTS[event]))
        return 0
    if "--base" in argv:
        base = argv[argv.index("--base") + 1]
    else:
        base, why = AUDIT.branch_base(str(ROOT))
        if base is None:
            return check_skip.unverified(NAME, why)
    try:
        ok, msg = judge(ROOT, base)
    except (git_history.HistoryUnknown, RuntimeError, OSError) as e:
        return check_skip.unverified(NAME, "cannot read the branch here: %s" % e)
    print(msg, file=sys.stdout if ok else sys.stderr)
    return 0 if ok else 1


def self_test():
    """The cases on a scratch repository: untouched, edited, regenerated,
    checked, checked-then-moved, and an unlisted file."""
    env = nros_clear_inherited_git_env(dict(os.environ))
    failures = 0
    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp) / "repo"
        repo.mkdir()

        def git(*args):
            subprocess.run(["git", "-C", str(repo), "-c", "user.name=t", "-c", "user.email=t@t"]
                           + list(args), check=True, capture_output=True, env=env)

        def write(rel, text):
            p = repo / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
            git("add", rel)

        def commit(msg):
            git("commit", "-qm", msg)
            return subprocess.run(["git", "-C", str(repo), "rev-parse", "HEAD"], check=True,
                                  capture_output=True, text=True, env=env).stdout.strip()

        git("init", "-q", "-b", "main")
        write("zephyr/Kconfig", "k1\n")
        write("README", "r\n")
        write(FIX.FIXTURE + "/" + FIX.INPUTS, "# header\nzephyr/Kconfig\n")
        write(FIX.FIXTURE + "/CMakeCache.txt", "NROS_RESOLVED_X:STRING=1\n")
        base = commit("base")

        saved = dict(os.environ)
        os.environ.clear()
        os.environ.update(env)
        try:
            def case(name, want_ok, head="HEAD"):
                nonlocal failures
                ok, msg = judge(repo, base, head)
                if ok != want_ok:
                    print("self-test: %s: judged %s, want %s\n%s" % (
                        name, "OK" if ok else "REFUSED", "OK" if want_ok else "REFUSED", msg))
                    failures += 1

            write("README", "r2\n")
            commit("unlisted")
            case("an unlisted file", True)

            write("zephyr/Kconfig", "k2\n")
            edited = commit("edit an input")
            case("an edited input, no snapshot, no trailer", False)

            want = FIX.digest(str(repo), ["zephyr/Kconfig"], "HEAD")
            git("commit", "-q", "--allow-empty", "-m", "x\n\n%s: checked %s" % (FIX.TRAILER, want))
            case("an edited input with a matching trailer", True)

            write("zephyr/Kconfig", "k3\n")
            commit("edit it again")
            case("a trailer whose digest an later edit moved", False)

            git("checkout", "-q", "-b", "regen", edited)
            write(FIX.FIXTURE + "/CMakeCache.txt", "NROS_RESOLVED_X:STRING=2\n")
            commit("regen")
            case("an edited input with the snapshot regenerated", True)

            git("checkout", "-q", "-b", "nolist", base)
            git("rm", "-q", FIX.FIXTURE + "/" + FIX.INPUTS)
            commit("drop the list")
            case("no inputs.txt", False)
        finally:
            os.environ.clear()
            os.environ.update(saved)
    if FIX.main(["knob-delivery-fixture.py", "--self-test"]) != 0:
        failures += 1
    print("%s self-test: %s" % (NAME, "FAILED (%d)" % failures if failures else "ok"))
    return 1 if failures else 0


def main(argv):
    if argv == ["--self-test"]:
        return self_test()
    if argv and argv[0] != "--base":
        print(__doc__.strip().splitlines()[-2:], file=sys.stderr)
        return 2
    rc = self_test()
    if rc != 0:
        return rc
    return require(argv)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
