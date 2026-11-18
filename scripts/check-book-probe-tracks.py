#!/usr/bin/env python3
"""Every book probe track still EXTRACTS — phase-517 W10.

`just probe <platform>` replays a chapter's `probe=NN` blocks in a container,
tens of minutes per chapter, so it runs weekly (`weekly.yml`) and not
per push. What CAN be checked per push, for free, is that each track's script
still assembles: the chapter's tags survive an edit, every `probe-run` step
keeps the `probe-expect` output block it is asserted against (and every such
block keeps its step), a `variant=` still selects something, the setup
placeholder the harness substitutes still matches once, and the result parses
(`bash -n`). Each of those breaking is otherwise discovered a week later, after
an install, as a track that fails for a reason that is not about the chapter.

The track list is READ from the generated probe-tracks.json (the board
registry's `[[probe]]` entries, phase-522 W2), the same list the harness and
the weekly workflow read, so a track added there is checked here without
editing this file.

Self-test (every call): a chapter whose run step lost its expected-output block
must fail to extract — the property this gate leans on.

Non-tip rehearsal (phase-567 W3c, every call): the release workflow checks out
the commit it releases BY SHA — a detached HEAD, possibly behind `main`, and
with no local `main` at all. The probe once read `main:` there and the first
rehearsal died on `fatal: invalid object name 'main'`; where `main` existed it
would have cloned main's TIP instead of the commit under test. So this builds
that shape in a scratch clone — HEAD detached on commit C1, a local `main` one
commit AHEAD whose tree has no release workflow and no install.sh — and
asserts the harness extracts the pinned-newer track from C1, refuses the
retired `PROBE_BRANCH`, and that the clone the book script and the release
builder run lands on C1. Negative control: `main:` must fail to read in that
clone, or the trap is not armed and the check proves nothing.

Run:  python3 scripts/check-book-probe-tracks.py
"""

import os
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
HARNESS = REPO / "scripts/probe/run-bootstrap-probe.sh"
EXTRACTOR = REPO / "scripts/probe/extract-book-steps.py"
# phase-522 W2 — the track list is generated from the board registry; this is
# the same reader the harness and weekly.yml use.
TRACK_LIST = REPO / "scripts/probe/probe-track.py"
sys.path.insert(0, str(REPO / "scripts/lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402


def platforms() -> list[str]:
    r = subprocess.run([sys.executable, str(TRACK_LIST), "--list"],
                       capture_output=True, text=True)
    rows = r.stdout.split()
    if r.returncode != 0 or "linux" not in rows or len(rows) < 2:
        sys.exit(f"check-book-probe-tracks: read {rows} from {TRACK_LIST.name} "
                 f"(rc={r.returncode}): {r.stderr.strip()}")
    return rows


def extract(platform: str, out: Path) -> subprocess.CompletedProcess:
    env = dict(os.environ, PROBE_TRACK="installed", PROBE_PLATFORM=platform,
               PROBE_EXTRACT_ONLY=str(out))
    return subprocess.run(["bash", str(HARNESS)], env=env, cwd=REPO,
                          capture_output=True, text=True)


def selftest() -> None:
    with tempfile.TemporaryDirectory() as d:
        ch = Path(d) / "chapter.md"
        ch.write_text("```bash probe=30 probe-run=10\nqemu-system-arm -kernel x\n```\n")
        r = subprocess.run([sys.executable, str(EXTRACTOR), "--out", str(Path(d) / "o.sh"),
                            str(ch)], capture_output=True, text=True)
        if r.returncode == 0 or "probe-expect" not in r.stderr:
            sys.exit("check-book-probe-tracks: SELFTEST FAILED — a run step with no "
                     f"expected-output block extracted (rc={r.returncode}): {r.stderr}")
    _selftest_drifted_submodule()
    # phase-537 W2: a language-tabbed chapter extracts one language per track.
    r = subprocess.run([sys.executable, str(EXTRACTOR), "--self-test"], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"check-book-probe-tracks: SELFTEST FAILED — the extractor's own: {r.stderr}{r.stdout}")


def _git(*args, cwd, env) -> str:
    r = subprocess.run(["git", *args], cwd=cwd, env=env, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"check-book-probe-tracks: non-tip rehearsal setup: git {' '.join(args)} "
                 f"(rc={r.returncode}): {r.stderr.strip()}")
    return r.stdout.strip()


def _clone_at(url: str, sha: str, dest: Path, env: dict, depth: str = "") -> subprocess.CompletedProcess:
    helper = REPO / "scripts/probe/commit-under-test.sh"
    clear = REPO / "scripts/lib/git-hook-env.sh"
    cmd = (f". {shlex.quote(str(clear))} && . {shlex.quote(str(helper))} && probe_clone_at_commit "
           f"{shlex.quote(url)} {sha} {shlex.quote(str(dest))} {depth}")
    return subprocess.run(["sh", "-c", cmd], env=env, capture_output=True, text=True)


def edited_paths(src: Path, env: dict) -> list[str]:
    """The FILES edited in `src` against its HEAD, plus untracked probe sources.

    `--ignore-submodules=all`: a submodule checked out away from its recorded
    pin (common after a rebase across a pin move) shows in a plain
    `git diff --name-only HEAD` as its gitlink path, a DIRECTORY. The overlay
    copies files; a gitlink is not one, and the depth-1 clone holds it as an
    empty directory, so overlaying it crashed with IsADirectoryError.
    """
    changed = _git("diff", "--name-only", "--ignore-submodules=all", "HEAD",
                   cwd=src, env=env).splitlines()
    changed += _git("ls-files", "--others", "--exclude-standard", "--",
                    "scripts/probe", "scripts/lib", cwd=src, env=env).splitlines()
    return changed


def overlay(src: Path, dst_root: Path, paths: list[str]) -> None:
    """Copy each edited file onto the clone, and delete each deleted one.

    A path that is a directory on either side is never a file edit (a gitlink
    `edited_paths` should already have dropped), so it is skipped, never
    unlinked."""
    for rel in paths:
        s, d = src / rel, dst_root / rel
        if s.is_dir() or d.is_dir():
            continue
        if s.is_file():
            d.parent.mkdir(parents=True, exist_ok=True)
            d.write_bytes(s.read_bytes())
            d.chmod(s.stat().st_mode)
        elif d.exists() or d.is_symlink():
            d.unlink()


def _selftest_drifted_submodule() -> None:
    """A submodule checked out off its pin is not an overlay path (the crash
    that `IsADirectoryError ... zpico-sys/zenoh-pico` reported)."""
    env = nros_clear_inherited_git_env(dict(os.environ))
    env.update(GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t", GIT_COMMITTER_NAME="t",
               GIT_COMMITTER_EMAIL="t@t", GIT_CONFIG_NOSYSTEM="1",
               GIT_CONFIG_GLOBAL=os.devnull)
    with tempfile.TemporaryDirectory() as d:
        d = Path(d)
        sub, sup, clone = d / "sub", d / "sup", d / "clone"
        for r in (sub, sup):
            r.mkdir()
            _git("init", "-q", "-b", "main", cwd=r, env=env)
        (sub / "a").write_text("1")
        _git("add", "a", cwd=sub, env=env)
        _git("commit", "-q", "-m", "s1", cwd=sub, env=env)
        (sup / "f.txt").write_text("old")
        _git("add", "f.txt", cwd=sup, env=env)
        _git("-c", "protocol.file.allow=always", "submodule", "add", "-q", str(sub), "dep",
             cwd=sup, env=env)
        _git("commit", "-q", "-m", "pin", cwd=sup, env=env)
        # drift: the submodule's checkout moves off the recorded pin
        (sup / "dep" / "a").write_text("2")
        _git("commit", "-q", "-am", "s2", cwd=sup / "dep", env=env)
        (sup / "f.txt").write_text("new")
        plain = _git("diff", "--name-only", "HEAD", cwd=sup, env=env).splitlines()
        if "dep" not in plain:
            sys.exit("check-book-probe-tracks: SELFTEST FAILED — negative control: the drifted "
                     f"submodule does not show in a plain diff ({plain}), so this case proves nothing")
        got = edited_paths(sup, env)
        if got != ["f.txt"]:
            sys.exit(f"check-book-probe-tracks: SELFTEST FAILED — edited_paths over a drifted "
                     f"submodule: want ['f.txt'], got {got}")
        (clone / "dep").mkdir(parents=True)  # a depth-1 clone's empty gitlink dir
        try:
            overlay(sup, clone, plain)       # even the unfiltered list must not crash
        except OSError as e:
            sys.exit(f"check-book-probe-tracks: SELFTEST FAILED — overlay on a gitlink: {e!r}")
        if (clone / "f.txt").read_text() != "new" or not (clone / "dep").is_dir():
            sys.exit("check-book-probe-tracks: SELFTEST FAILED — overlay did not copy the file "
                     "or removed the gitlink directory")


def _clone_scratch_root() -> "str | None":
    """Where the rehearsal's clones go: tmpfs (`/dev/shm`) when this host has
    a writable one, else `/tmp`. Not `TMPDIR`.

    Each clone fetches and checks out the whole HEAD tree (about 15,000 files,
    120 MB), three times a run. On a checkout on a slow disk that was nearly
    all of the run: about 111 s a shape on a USB disk under IO pressure. The
    hook gate's `tmpdir` shape points TMPDIR inside the checkout on purpose.
    The selftest's own repositories stay on TMPDIR, so that shape still probes
    them there; these clones need no particular disk to prove what they
    prove."""
    for root in ("/dev/shm", "/tmp"):
        if not (Path(root).is_dir() and os.access(root, os.W_OK)):
            continue
        # A run holds about 400 MB there at its peak (measured on host 243:
        # 1.16 GB for the hook gate's three shapes at once). A tmpfs too small
        # for that would fail the clone, so it is used only with room to spare.
        st = os.statvfs(root)
        if root == "/dev/shm" and st.f_bavail * st.f_frsize < 1 << 30:
            continue
        return root
    return None  # neither: tempfile's own default


def non_tip_rehearsal() -> list[str]:
    """The release rehearsal's checkout shape (see the module doc). Returns problems."""
    env = nros_clear_inherited_git_env(dict(os.environ))
    for k in [k for k in env if k.startswith("PROBE_")]:
        del env[k]
    env.update(GIT_AUTHOR_NAME="probe", GIT_AUTHOR_EMAIL="probe@invalid",
               GIT_COMMITTER_NAME="probe", GIT_COMMITTER_EMAIL="probe@invalid")
    head = _git("rev-parse", "HEAD", cwd=REPO, env=env)
    common = _git("rev-parse", "--path-format=absolute", "--git-common-dir", cwd=REPO, env=env)
    bad = []
    with tempfile.TemporaryDirectory(prefix="nros-book-probe-", dir=_clone_scratch_root()) as d:
        d = Path(d)
        repo = d / "repo"
        # This checkout's HEAD by SHA, depth 1 — in a linked worktree that SHA
        # is not advertised by the shared git dir, the case a ref clone misses.
        r = _clone_at(f"file://{common}", head, repo, env, "1")
        if r.returncode != 0:
            return [f"probe_clone_at_commit could not fetch HEAD {head[:10]} by SHA: {r.stderr.strip()}"]
        # Overlay what is being edited, so the check probes this tree.
        overlay(REPO, repo, edited_paths(REPO, env))
        _git("add", "-A", ".", cwd=repo, env=env)
        _git("commit", "-q", "--no-verify", "--allow-empty", "-m", "C1: the commit under test", cwd=repo, env=env)
        c1 = _git("rev-parse", "HEAD", cwd=repo, env=env)
        # C2, ahead of C1 on a local `main`: a tree a `main:` read cannot use.
        _git("rm", "-q", ".github/workflows/release-nros.yml", "scripts/install.sh", cwd=repo, env=env)
        _git("commit", "-q", "--no-verify", "-m", "C2: main moved on", cwd=repo, env=env)
        _git("branch", "-f", "main", "HEAD", cwd=repo, env=env)
        _git("checkout", "-q", "--detach", c1, cwd=repo, env=env)
        armed = subprocess.run(["git", "show", "main:.github/workflows/release-nros.yml"],
                               cwd=repo, env=env, capture_output=True, text=True)
        if armed.returncode == 0:
            return ["NEGATIVE CONTROL FAILED: `main:` still reads the release workflow, "
                    "so the non-tip shape is not built and this check proves nothing"]
        harness = repo / "scripts/probe/run-bootstrap-probe.sh"
        out = d / "pinned-newer.sh"
        r = subprocess.run(["bash", str(harness)], cwd=repo, capture_output=True, text=True,
                           env=dict(env, PROBE_TRACK="installed", PROBE_PLATFORM="pinned-newer",
                                    PROBE_EXTRACT_ONLY=str(out)))
        if r.returncode != 0:
            bad.append("pinned-newer on a detached non-tip SHA did not extract "
                       f"(rc={r.returncode}): {(r.stderr or r.stdout).strip()}")
        r = subprocess.run(["bash", str(harness)], cwd=repo, capture_output=True, text=True,
                           env=dict(env, PROBE_BRANCH="main", PROBE_TRACK="installed",
                                    PROBE_PLATFORM="pinned-newer", PROBE_EXTRACT_ONLY=str(out)))
        if r.returncode != 2 or "PROBE_BRANCH is retired" not in r.stderr:
            bad.append(f"PROBE_BRANCH=main was not refused as retired (rc={r.returncode}): "
                       f"{(r.stderr or r.stdout).strip()}")
        book = d / "checkout.sh"
        r = subprocess.run(["bash", str(harness)], cwd=repo, capture_output=True, text=True,
                           env=dict(env, PROBE_EXTRACT_ONLY=str(book)))
        want = 'probe_clone_at_commit "$PROBE_CLONE_URL" "$PROBE_COMMIT" nano-ros\n'
        if r.returncode != 0:
            bad.append(f"the checkout track did not extract on a detached SHA (rc={r.returncode}): "
                       f"{(r.stderr or r.stdout).strip()}")
        elif want not in book.read_text():
            bad.append("the checkout track's clone line is not a fetch of $PROBE_COMMIT")
        # The clones themselves, from the non-tip clone: the book's (plain path,
        # full) and the release builder's (file://, depth 1). Each must land on C1.
        for name, url, depth in (("book", str(repo / ".git"), ""),
                                 ("release builder", f"file://{repo / '.git'}", "1")):
            dest = d / f"clone-{name.replace(' ', '-')}"
            r = _clone_at(url, c1, dest, env, depth)
            got = subprocess.run(["git", "rev-parse", "HEAD"], cwd=dest, env=env,
                                 capture_output=True, text=True).stdout.strip() if dest.exists() else ""
            if r.returncode != 0 or got != c1:
                bad.append(f"the {name} clone did not land on the commit under test "
                           f"{c1[:10]} (got {got[:10] or 'nothing'}): {r.stderr.strip()}")
    return bad


def main(argv: list[str]) -> int:
    selftest()
    rehearsal = non_tip_rehearsal()
    if rehearsal:
        print("check-book-probe-tracks: the probe does not test the commit under test "
              "on a detached non-tip SHA (phase-567 W3c):", file=sys.stderr)
        for b in rehearsal:
            print(f"  {b}", file=sys.stderr)
        return 1
    if argv == ["--self-test"]:
        # Every repository this script builds (the selftest's and the
        # rehearsal's clones), and none of the 78 chapter extractions, which
        # build none. `check-hook-repo-side-effects` probes this arm in three
        # repo shapes; the extractions were most of its per-shape cost.
        print("check-book-probe-tracks: self-test OK (the selftest and the non-tip "
              "rehearsal; chapters not extracted)")
        return 0
    if argv:
        print(f"check-book-probe-tracks: unknown arguments {argv}; the only one is "
              "--self-test", file=sys.stderr)
        return 2
    failed = []
    names = platforms()
    with tempfile.TemporaryDirectory() as d:
        for p in names:
            r = extract(p, Path(d) / f"{p}.sh")
            if r.returncode != 0:
                failed.append((p, (r.stderr or r.stdout).strip()))
    if failed:
        print(f"check-book-probe-tracks: {len(failed)} track(s) no longer extract:",
              file=sys.stderr)
        for p, why in failed:
            print(f"  just probe {p}:\n    " + why.replace("\n", "\n    "), file=sys.stderr)
        return 1
    print(f"check-book-probe-tracks: OK ({len(names)} installed-track chapter(s) extract; "
          "a detached non-tip SHA is the commit under test)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
