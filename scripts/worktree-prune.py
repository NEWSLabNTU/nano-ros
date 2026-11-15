#!/usr/bin/env python3
"""List, and with --apply remove, YOUR worktrees whose work is already on main.

Issue 2611 (phase-489). Agent sessions here each make linked worktrees, one
per task, and a worktree that has run `just ci gate` holds 50-300 GB of build
output. When its PR merges nothing removes it, so finished worktrees fill the
host: /home reached 100 % on 2026-11-15, with 11 GB free, and gates went red
on their disk floor. This verb removes them. It is careful in six ways, and
each one has a negative control in --self-test:

  OWNERSHIP   only the caller's own worktrees. Git records no creator, so
              ownership is a MARKER: `nros-owner` in the worktree's private git
              dir (`.git/worktrees/<name>/`, never in the tree, never
              committed), written automatically by `just setup-worktree`
              when absent, or by `--mark <path>`. The caller is
              `$NROS_WORKTREE_OWNER`, else `$CLAUDE_CODE_SESSION_ID`. No marker,
              or another owner's, means LISTED AND SKIPPED, never removed: an
              ownership that cannot be proven is not assumed.
  MERGED      every commit on the worktree's HEAD is already in `origin/main`
              by patch-id (`git cherry`, so a queue rebase-merge counts), or
              the branch's PR is merged and every commit it has is on a remote.
  CLEAN       no tracked change and no untracked file. A nested repository
              (a submodule, or a provisioned source checkout such as
              `third-party/nuttx/libc`) counts as clean only if it is clean
              itself and its HEAD is on one of ITS remotes.
  PUSHED      no commit on HEAD that is not on a remote, unless it is already
              in main by patch-id.
  FORCE       `git worktree remove` without --force first. --force only when
              git refused because of nested repositories, and only after the
              three checks above passed: it is how git removes a worktree that
              holds submodules, not a way past unsaved work.
  NEVER       `git submodule deinit` (issue 2505), the main checkout, the
              worktree you are standing in, or a locked worktree.

Dry run by default.

  worktree-prune.py                    list every linked worktree and what would happen
  worktree-prune.py --apply            remove the ones that qualify
  worktree-prune.py --mark <path>      record the caller as the owner of <path>
  worktree-prune.py --mark-if-absent <path>
                                       the same, only if <path> has no owner yet; never fails
                                       (`just setup-worktree` runs it, so a worktree is marked
                                       by the session that provisions it)
  worktree-prune.py --reminder         one line, only if a removable worktree is over 10 GB (the gate's)
  worktree-prune.py --self-test        the negative controls, on scratch repositories
  worktree-prune.py --gate             self-test, then the wiring above is still in place

Every invocation runs the self-test first and refuses to act if it fails.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

MARKER = "nros-owner"
REMINDER_BYTES = 10 * 1024**3
DU_TIMEOUT_S = 30


def git(args, cwd, check=True):
    p = subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True)
    if check and p.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} in {cwd}: {p.stderr.strip()}")
    return p


def caller_id():
    return os.environ.get("NROS_WORKTREE_OWNER") or os.environ.get("CLAUDE_CODE_SESSION_ID") or ""


def worktrees(repo):
    """Linked worktrees of `repo`'s common dir, main checkout excluded."""
    out = git(["worktree", "list", "--porcelain"], repo).stdout
    items, cur = [], {}
    for line in out.splitlines() + [""]:
        if not line:
            if cur:
                items.append(cur)
            cur = {}
            continue
        key, _, val = line.partition(" ")
        cur[key] = val or True
    return items[1:]


def private_git_dir(path):
    return Path(git(["rev-parse", "--absolute-git-dir"], path).stdout.strip())


def owner_of(path):
    try:
        return (private_git_dir(path) / MARKER).read_text().strip()
    except (OSError, RuntimeError):
        return ""


def unpushed(path):
    """Commits on HEAD that no remote-tracking ref has."""
    p = git(["rev-list", "HEAD", "--not", "--remotes"], path, check=False)
    if p.returncode != 0:
        return ["<HEAD unreadable>"]
    return p.stdout.split()


def nested_problem(path):
    """Why the nested repository at `path` is not safe to delete, or None."""
    st = git(["status", "--porcelain=v1", "--ignore-submodules=none"], path, check=False)
    if st.returncode != 0:
        return "status unreadable"
    if st.stdout.strip():
        return "has uncommitted changes"
    if unpushed(path):
        return "HEAD is on none of its remotes"
    return None


def dirt(path):
    """(problems, nested) for a worktree: what makes it unsafe, and the
    nested repositories that are the only reason a plain remove would refuse."""
    st = git(["status", "--porcelain=v1", "--ignore-submodules=none"], path, check=False)
    if st.returncode != 0:
        return [f"status failed: {st.stderr.strip()}"], []
    gitlinks = set(
        l.split("\t", 1)[1]
        for l in git(["ls-files", "-s"], path).stdout.splitlines()
        if l.startswith("160000 ")
    )
    problems, nested = [], []
    for line in st.stdout.splitlines():
        code, rel = line[:2], line[3:].rstrip("/")
        full = Path(path) / rel
        if rel in gitlinks or (code == "??" and (full / ".git").exists()):
            why = nested_problem(full)
            if why:
                problems.append(f"nested repo {rel} {why}")
            else:
                nested.append(rel)
        else:
            problems.append(f"unsaved change: {line}")
    # A submodule at its pin and clean prints nothing, but still makes a plain
    # `git worktree remove` refuse.
    for rel in gitlinks:
        if (Path(path) / rel / ".git").exists() and rel not in nested and not any(rel in p for p in problems):
            why = nested_problem(Path(path) / rel)
            if why:
                problems.append(f"nested repo {rel} {why}")
            else:
                nested.append(rel)
    return problems, nested


def merged_prs():
    """{branch: pr number} for merged PRs. Tests inject it; production asks gh."""
    inj = os.environ.get("NROS_WORKTREE_PRUNE_MERGED_PRS")
    if inj is not None:
        return json.loads(inj)
    if not shutil.which("gh"):
        return {}
    p = subprocess.run(
        ["gh", "pr", "list", "--state", "merged", "--author", "@me", "--limit", "200",
         "--json", "number,headRefName"],
        capture_output=True, text=True,
    )
    if p.returncode != 0:
        return {}
    return {r["headRefName"]: r["number"] for r in json.loads(p.stdout)}


def merged_reason(path, branch, base, prs):
    plus = [l for l in git(["cherry", base, "HEAD"], path).stdout.splitlines() if l.startswith("+")]
    if not plus:
        return f"fully in {base}", None
    if branch and branch in prs:
        left = unpushed(path)
        if left:
            return None, f"PR #{prs[branch]} merged, but {len(left)} commit(s) are on no remote"
        return f"PR #{prs[branch]} merged", None
    return None, f"{len(plus)} commit(s) not in {base}"


def size_of(path):
    try:
        p = subprocess.run(["du", "-sx", "--block-size=1", str(path)],
                           capture_output=True, text=True, timeout=DU_TIMEOUT_S)
        return int(p.stdout.split()[0]), False
    except subprocess.TimeoutExpired:
        return None, True
    except (ValueError, IndexError):
        return None, False


def human(n):
    return "?" if n is None else f"{n / 1024**3:.1f} GB"


def assess(repo, base, me, prs, here):
    """One row per linked worktree: (path, verdict, reason, nested)."""
    rows = []
    for wt in worktrees(repo):
        path = wt["worktree"]
        branch = wt.get("branch", "").removeprefix("refs/heads/") if wt.get("branch") else ""
        if "prunable" in wt or not Path(path).is_dir():
            rows.append((path, "skip", "directory is gone (`git worktree prune` forgets it)", []))
            continue
        if "locked" in wt:
            rows.append((path, "skip", "locked", []))
            continue
        if here and os.path.realpath(path) == os.path.realpath(here):
            rows.append((path, "skip", "this is the worktree you are in", []))
            continue
        owner = owner_of(path)
        if not me:
            rows.append((path, "skip", "no caller identity (set NROS_WORKTREE_OWNER)", []))
            continue
        if not owner:
            rows.append((path, "skip", "ownership unproven (no nros-owner marker)", []))
            continue
        if owner != me:
            rows.append((path, "skip", f"owned by {owner}", []))
            continue
        ok, why = merged_reason(path, branch, base, prs)
        if not ok:
            rows.append((path, "keep", why, []))
            continue
        problems, nested = dirt(path)
        if problems:
            rows.append((path, "skip", f"{ok}, but " + "; ".join(problems[:3]), []))
            continue
        rows.append((path, "remove", ok, nested))
    return rows


def remove(repo, path, nested):
    p = git(["worktree", "remove", path], repo, check=False)
    if p.returncode == 0:
        return True, "removed"
    if nested and ("submodule" in p.stderr or "untracked" in p.stderr or "modified" in p.stderr):
        # Only reached after assess() proved every nested repo clean and pushed
        # and the worktree itself clean: --force here deletes nothing unsaved.
        f = git(["worktree", "remove", "--force", path], repo, check=False)
        if f.returncode == 0:
            return True, f"removed (--force: nested repos {', '.join(nested)})"
        return False, f.stderr.strip()
    return False, p.stderr.strip()


def common_root(cwd):
    common = git(["rev-parse", "--path-format=absolute", "--git-common-dir"], cwd).stdout.strip()
    return str(Path(common).parent)


def run(argv, cwd=None):
    cwd = cwd or os.getcwd()
    base = "origin/main"
    if "--base" in argv:
        base = argv[argv.index("--base") + 1]
    me = caller_id()
    if "--mark-if-absent" in argv:
        target = argv[argv.index("--mark-if-absent") + 1]
        try:
            top = git(["rev-parse", "--show-toplevel"], target).stdout.strip()
            if os.path.realpath(top) == os.path.realpath(common_root(target)):
                return 0  # the main checkout is nobody's to prune
            have = owner_of(target)
            if have:
                return 0  # already owned; never re-attributed
            if not me:
                print("  [NOTE]    worktree not marked as yours: no caller identity "
                      "(set NROS_WORKTREE_OWNER; issue 2611)")
                return 0
            (private_git_dir(target) / MARKER).write_text(me + "\n")
            print(f"  [MARK]    this worktree is owned by {me} (just worktree-prune; issue 2611)")
        except (OSError, RuntimeError) as e:
            print(f"  [NOTE]    worktree owner not recorded: {e}")
        return 0
    if "--mark" in argv:
        target = argv[argv.index("--mark") + 1]
        if not me:
            print("worktree-prune: no caller identity; set NROS_WORKTREE_OWNER", file=sys.stderr)
            return 2
        have = owner_of(target)
        if have and have != me:
            print(f"worktree-prune: {target} is already owned by {have}; not re-marked", file=sys.stderr)
            return 1
        if os.path.realpath(git(["rev-parse", "--show-toplevel"], target).stdout.strip()) \
                == os.path.realpath(common_root(target)):
            print("worktree-prune: that is the main checkout, which is nobody's to prune", file=sys.stderr)
            return 1
        (private_git_dir(target) / MARKER).write_text(me + "\n")
        print(f"worktree-prune: {target} is owned by {me}")
        return 0
    repo = common_root(cwd)
    here = git(["rev-parse", "--show-toplevel"], cwd).stdout.strip()
    rows = assess(repo, base, me, merged_prs(), here)
    if "--reminder" in argv:
        big = []
        for path, verdict, _, _ in rows:
            if verdict != "remove":
                continue
            n, slow = size_of(path)
            if slow or (n or 0) >= REMINDER_BYTES:
                big.append(f"{Path(path).name} {'>' if slow else ''}{human(n) if n else '?'}")
        if big:
            print(f"ci gate: {len(big)} merged worktree(s) of yours over 10 GB ({', '.join(big)}): "
                  "`just worktree-prune` lists them, `--apply` removes them.")
        return 0
    apply = "--apply" in argv
    if not rows:
        print("worktree-prune: no linked worktrees")
    failed = 0
    for path, verdict, reason, nested in rows:
        if verdict == "remove":
            n, slow = size_of(path)
            sz = f">{DU_TIMEOUT_S}s to count" if slow else human(n)
            if apply:
                ok, msg = remove(repo, path, nested)
                failed += not ok
                print(f"  {'REMOVED' if ok else 'FAILED '} {path}  [{sz}]  {reason}: {msg}")
            else:
                extra = f" (needs --force: nested {', '.join(nested)})" if nested else ""
                print(f"  WOULD REMOVE {path}  [{sz}]  {reason}{extra}")
        else:
            print(f"  {verdict.upper():12} {path}  {reason}")
    if not apply and any(r[1] == "remove" for r in rows):
        print("worktree-prune: dry run; `just worktree-prune --apply` removes the WOULD REMOVE rows")
    return 1 if failed else 0


# ------------------------------------------------------------------ self-test

def _sh(args, cwd, env):
    p = subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"{' '.join(args)}: {p.stderr}")
    return p.stdout


def quiet(fn, *a, **k):
    import contextlib, io
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
        return fn(*a, **k)


def self_test():
    env = nros_clear_inherited_git_env(dict(os.environ))
    env.update(GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t", GIT_COMMITTER_NAME="t",
               GIT_COMMITTER_EMAIL="t@t", GIT_CONFIG_NOSYSTEM="1", HOME=tempfile.mkdtemp())
    saved = dict(os.environ)
    os.environ.clear()
    os.environ.update(env)
    fails = []
    try:
        with tempfile.TemporaryDirectory() as t:
            t = Path(t)
            G = lambda *a, cwd: _sh(["git", "-c", "protocol.file.allow=always", *a], cwd, env)

            def commit(cwd, name, text="x"):
                (Path(cwd) / name).write_text(text)
                G("add", name, cwd=cwd)
                G("commit", "-q", "-m", name, cwd=cwd)

            # upstream + a submodule source
            G("init", "-q", "--bare", "-b", "main", str(t / "origin.git"), cwd=t)
            G("init", "-q", "--bare", "-b", "main", str(t / "sub.git"), cwd=t)
            G("clone", "-q", str(t / "sub.git"), str(t / "subw"), cwd=t)
            commit(t / "subw", "s")
            G("push", "-q", "origin", "HEAD:main", cwd=t / "subw")
            G("clone", "-q", str(t / "origin.git"), str(t / "main"), cwd=t)
            m = t / "main"
            commit(m, "base")
            G("push", "-q", "origin", "HEAD:main", cwd=m)

            def wt(name, base="origin/main"):
                p = t / name
                G("worktree", "add", "-q", "-b", name, str(p), base, cwd=m)
                return p

            def merge_to_main(src):
                # what the queue does: a rebase-merge, new SHAs, same patches
                G("fetch", "-q", "origin", cwd=m)
                G("checkout", "-q", "--detach", "origin/main", cwd=m)
                for c in reversed(G("rev-list", "origin/main.." + src, cwd=m).split()):
                    G("cherry-pick", "-x", c, cwd=m)  # -x: new SHA, same patch-id
                G("push", "-q", "origin", "HEAD:main", cwd=m)
                G("fetch", "-q", "origin", cwd=m)

            os.environ["NROS_WORKTREE_OWNER"] = "me"
            os.environ["NROS_WORKTREE_PRUNE_MERGED_PRS"] = json.dumps({"prmerged": 7, "prunpushed": 8})

            cases = {}
            # 1 owned, rebase-merged, clean -> remove
            p = wt("merged"); commit(p, "a"); G("push", "-q", "origin", "HEAD:merged", cwd=p)
            merge_to_main("merged"); cases["merged"] = "remove"
            # 2 merged, no marker -> skip
            p = wt("nomarker"); cases["nomarker"] = "skip:ownership unproven"
            # 3 merged, other owner -> skip
            p = wt("other"); (private_git_dir(p) / MARKER).write_text("someone\n"); cases["other"] = "skip:owned by someone"
            # 4 owned, merged, tracked edit -> skip
            p = wt("dirty"); (p / "base").write_text("edited"); cases["dirty"] = "skip:unsaved change"
            # 5 owned, merged, untracked plain file -> skip
            p = wt("untracked"); (p / "notes.txt").write_text("n"); cases["untracked"] = "skip:unsaved change"
            # 6 owned, unmerged unpushed commit -> keep
            p = wt("unmerged"); commit(p, "u"); cases["unmerged"] = "keep:not in origin/main"
            # 7 owned, merged then one more local commit -> keep
            p = wt("extra"); commit(p, "e1"); G("push", "-q", "origin", "HEAD:extra", cwd=p)
            merge_to_main("extra"); commit(p, "e2"); cases["extra"] = "keep:not in origin/main"
            # 8 owned, merged, clean nested provisioned repo -> remove (force)
            p = wt("nestok"); G("clone", "-q", str(t / "sub.git"), str(p / "vendor"), cwd=t); cases["nestok"] = "remove"
            # 9 nested repo with an unpushed commit -> skip
            p = wt("nestunpushed"); G("clone", "-q", str(t / "sub.git"), str(p / "vendor"), cwd=t)
            commit(p / "vendor", "local"); cases["nestunpushed"] = "skip:on none of its remotes"
            # 10 nested repo dirty -> skip
            p = wt("nestdirty"); G("clone", "-q", str(t / "sub.git"), str(p / "vendor"), cwd=t)
            (p / "vendor" / "s").write_text("edited"); cases["nestdirty"] = "skip:uncommitted changes"
            # 11 locked -> skip
            p = wt("locked"); G("worktree", "lock", str(p), cwd=m); cases["locked"] = "skip:locked"
            # 12 PR merged (squash-like: + commits) and pushed -> remove
            p = wt("prmerged"); commit(p, "pm"); G("push", "-q", "origin", "HEAD:prmerged", cwd=p); cases["prmerged"] = "remove"
            # 13 PR merged but a commit on no remote -> keep
            p = wt("prunpushed"); commit(p, "pu"); cases["prunpushed"] = "keep:on no remote"
            # 14 real submodule, at pin, clean -> remove (force)
            G("checkout", "-q", "--detach", "origin/main", cwd=m)
            G("submodule", "add", "-q", str(t / "sub.git"), "subm", cwd=m)
            G("commit", "-q", "-m", "submodule", cwd=m)
            G("push", "-q", "origin", "HEAD:main", cwd=m); G("fetch", "-q", "origin", cwd=m)
            p = wt("withsub"); G("submodule", "update", "-q", "--init", cwd=p); cases["withsub"] = "remove"
            # 15 submodule with a local edit -> skip
            p = wt("subdirty"); G("submodule", "update", "-q", "--init", cwd=p)
            (p / "subm" / "s").write_text("edited"); cases["subdirty"] = "skip:uncommitted changes"
            # everything except nomarker/other is ours
            for name in cases:
                if name not in ("nomarker", "other"):
                    (private_git_dir(t / name) / MARKER).write_text("me\n")

            def verdicts(me_override=None):
                if me_override is not None:
                    os.environ["NROS_WORKTREE_OWNER"] = me_override
                rows = assess(str(m), "origin/main", caller_id(), merged_prs(), str(m))
                os.environ["NROS_WORKTREE_OWNER"] = "me"
                return {Path(r[0]).name: r for r in rows}

            got = verdicts()
            for name, want in cases.items():
                v, _, frag = want.partition(":")
                r = got.get(name)
                if r is None or r[1] != v or (frag and frag not in r[2]):
                    fails.append(f"{name}: want {want}, got {r[1:3] if r else None}")

            # dry run removes nothing
            before = set(os.listdir(t))
            quiet(run, [], cwd=str(m))
            if set(os.listdir(t)) != before:
                fails.append("dry run removed something")
            # no identity -> nothing is ours
            os.environ["NROS_WORKTREE_OWNER"] = ""
            saved_sid = os.environ.pop("CLAUDE_CODE_SESSION_ID", None)
            if any(r[1] == "remove" for r in verdicts("").values()):
                fails.append("no caller identity still found a worktree to remove")
            if saved_sid:
                os.environ["CLAUDE_CODE_SESSION_ID"] = saved_sid
            # --mark refuses another owner's worktree and the main checkout
            if quiet(run, ["--mark", str(t / "other")], cwd=str(m)) == 0:
                fails.append("--mark took over another owner's worktree")
            if quiet(run, ["--mark", str(m)], cwd=str(m)) == 0:
                fails.append("--mark marked the main checkout")
            # --mark-if-absent: marks an unmarked worktree, never re-attributes
            # one, never marks the main checkout, needs an identity
            quiet(run, ["--mark-if-absent", str(t / "nomarker")], cwd=str(m))
            if owner_of(t / "nomarker") != "me":
                fails.append("--mark-if-absent did not mark an unmarked worktree")
            (private_git_dir(t / "nomarker") / MARKER).unlink()
            quiet(run, ["--mark-if-absent", str(t / "other")], cwd=str(m))
            if owner_of(t / "other") != "someone":
                fails.append("--mark-if-absent re-attributed another owner's worktree")
            quiet(run, ["--mark-if-absent", str(m)], cwd=str(m))
            if (private_git_dir(m) / MARKER).exists():
                fails.append("--mark-if-absent marked the main checkout")
            os.environ["NROS_WORKTREE_OWNER"] = ""
            sid = os.environ.pop("CLAUDE_CODE_SESSION_ID", None)
            quiet(run, ["--mark-if-absent", str(t / "nomarker")], cwd=str(m))
            if (private_git_dir(t / "nomarker") / MARKER).exists():
                fails.append("--mark-if-absent wrote a marker with no caller identity")
            os.environ["NROS_WORKTREE_OWNER"] = "me"
            if sid:
                os.environ["CLAUDE_CODE_SESSION_ID"] = sid
            # the worktree you are in is never removed
            here_rows = assess(str(m), "origin/main", "me", merged_prs(), str(t / "merged"))
            if any(Path(r[0]).name == "merged" and r[1] == "remove" for r in here_rows):
                fails.append("the caller's own current worktree was a removal candidate")
            # remove() itself never forces past a refusal that is not about
            # nested repos, even if a caller skipped assess()
            ok, _ = remove(str(m), str(t / "untracked"), [])
            if ok or not (t / "untracked").exists():
                fails.append("remove() forced away a worktree with an untracked file")
            # apply removes exactly the remove rows, and every skip survives
            quiet(run, ["--apply"], cwd=str(m))
            for name, want in cases.items():
                exists = (t / name).exists()
                if want == "remove" and exists:
                    fails.append(f"{name}: --apply left it")
                if want != "remove" and not exists:
                    fails.append(f"{name}: --apply REMOVED a worktree it had to keep")
            if (t / "subw").exists() is False or not (t / "sub.git").exists():
                fails.append("a submodule's source was touched")
            # no git argv in this file may carry deinit (issue 2505): look for
            # it as a quoted argument, which is the only way one could be passed
            quoted = chr(34) + "dein" + "it" + chr(34)
            if quoted in Path(__file__).read_text():
                fails.append("the script passes `deinit` to git (issue 2505)")
    finally:
        os.environ.clear()
        os.environ.update(saved)
    for f in fails:
        print(f"worktree-prune self-test FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("worktree-prune self-test: OK (15 worktree cases, dry run, identity, --mark, --mark-if-absent, current worktree, --apply)")
    return 0


WIRING = (
    ("justfile", "--mark-if-absent .", "`just setup-worktree` no longer marks the worktree it provisions"),
    ("just/ci.just", "worktree-prune.py --reminder", "`just ci gate` no longer prints the merged-worktree reminder"),
)


def wiring_problems(root):
    out = []
    for rel, needle, why in WIRING:
        try:
            if needle not in (Path(root) / rel).read_text():
                out.append(f"{rel}: {why} (no `{needle}`)")
        except OSError as e:
            out.append(f"{rel}: unreadable ({e})")
    return out


def main(argv):
    # The negative controls run on EVERY path (check-gate-selftests): a
    # removal tool whose refusals have decayed must not remove anything. They
    # take ~0.5 s on scratch repositories and touch nothing else.
    advisory = "--mark-if-absent" in argv or "--reminder" in argv
    rc = quiet(self_test) if (advisory or "--gate" not in argv) else self_test()
    if rc != 0:
        if advisory:
            print("  [NOTE]    worktree-prune's self-test failed; skipped (just check worktree-prune)")
            return 0
        print("worktree-prune: self-test FAILED; refusing to act (run --self-test for the cases)",
              file=sys.stderr)
        return 1
    if "--self-test" in argv:
        return 0
    if "--gate" in argv:
        probs = wiring_problems(Path(__file__).resolve().parent.parent)
        for p in probs:
            print(f"check-worktree-prune: {p}", file=sys.stderr)
        if probs:
            return 1
        print("check-worktree-prune: OK (self-test, setup-worktree marks, ci gate reminds)")
        return 0
    nros_clear_inherited_git_env()
    return run(argv)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
