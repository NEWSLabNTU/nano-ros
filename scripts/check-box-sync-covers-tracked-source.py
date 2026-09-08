#!/usr/bin/env python3
"""Every TRACKED source path must survive the ROS-box mirror — in EVERY repo.

`scripts/dev/ros2-box-sync.sh` excludes build-output directories by NAME
pattern, and the tree contains tracked SOURCE whose names match those patterns.
That collision has now eaten tracked files five times, each time discovered as a
build failure inside the box that named something else entirely:

    `build`        matched `scripts/build/`            -> lost cargo.sh
    `build-*`      matched `scripts/build/build-root.sh`
    (fix: anchor, then make every pattern directory-only)
    `build-*/`     matched `packages/cli/build-support/`
                   -> "couldn't read nros-cli-core/../build-support/
                       submodule_watch.rs", i.e. the box could not build `nros`
    `target/`      matched `zephyr/drivers/i2c/target/` (I2C target mode)
                   -> "drivers/i2c/Kconfig:101: 'drivers/i2c/target/Kconfig' not
                       found", i.e. the box could configure NO Zephyr image

Anchoring cured one instance and the trailing `/` cured another; neither cured
the CLASS, because a tracked DIRECTORY whose name begins with `build-` still
matches a directory-only pattern.

WHY THE FIFTH ONE GOT PAST THIS GATE

This file existed for the fourth incident and did not fire on the fifth. It
swept `git ls-files` — the SUPERPROJECT's index — and `zephyr-workspace/zephyr`
is a nested repository, west-managed and gitignored here. Measured on `main`
before the fix::

    $ git ls-files | grep -cE '(^|/)target/'      # 0
    $ find zephyr-workspace -type d -name target  # 2 (excluding build-*)

Zero versus two. rsync copies the WORKING TREE, so the mirror's contents are
not a property of one index; a sweep scoped to one repository is blind to every
sibling's source, and the blind spot covers ~39 repositories and the majority
of the bytes.

So the sweep now asks the question of every repository the mirror copies. That
is the contract: **a mirror is faithful when no repository's tracked source is
dropped**, not when the superproject's is. Which files a given build "needs" is
not knowable from here — faithful-copy is the rule that can actually be
enforced, and it is the rule the five incidents each violated.

WHAT THE WIDENED SWEEP FOUND IMMEDIATELY

A SECOND live instance, in a completely different tree:
`third-party/px4/PX4-Autopilot/boards/modalai/voxl2/target/` — six tracked
files (`voxl-px4`, `voxl-px4-start`, two `.config`s, …), referenced by PX4's own
`boards/modalai/voxl2/scripts/install-voxl.sh`, which `adb push`es each of them
by that path. Upstream source, dropped by a Rust build-output pattern, in a repo
this gate could not see. `zephyr-workspace/zephyr` was simply the one we hit
first.

WHEN A TREE IS NOT HERE (three outcomes, issue 1043's shape)

Agent worktrees and every CI lane carry neither `zephyr-workspace` nor an
initialised submodule, so the sweep would silently compare nothing. It says so
instead:

    FAIL          a tracked path was MEASURED to be dropped.
    NOT VERIFIED  a tree the sync script's own rules name, or a declared
                  submodule, is absent here. Reported by name, never silent,
                  and recorded in the check-skip ledger by the recipe.
    OK            measured, nothing dropped.

`NROS_BOX_SYNC_SWEEP_STRICT=1` turns a NOT VERIFIED into a failure, for a lane
that really does provision every tree. Do not set it on a lane that provisions
a subset — that reinstates the unconditional failure issue 1043 is about.

Source-only: no rsync, no box, no network. Cost is one pruned directory walk
(the walk skips anything the rules exclude, so the tree's ~40 GB of build output
is never visited) plus one `git ls-files` per repository.
"""

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SYNC = ROOT / "scripts/dev/ros2-box-sync.sh"

sys.path.insert(0, str(ROOT / "scripts/lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

# A component that cannot occur in a real path, appended to a DIRECTORY so the
# directory-only rules (which require more path after the match) evaluate as
# "this directory is an ancestor of something". See `dir_verdict`.
SENTINEL = "\x00"


def rules(text=None):
    """(kind, pattern) in file order — rsync takes the FIRST match."""
    out = []
    src = SYNC.read_text() if text is None else text
    for line in src.splitlines():
        m = re.match(r"\s*--(include|exclude)\s+'([^']+)'", line)
        if m:
            out.append((m.group(1), m.group(2)))
    return out


def matches(pattern, path):
    """Approximate rsync matching for the shapes this file uses.

    DIRECTORY-ONLY is the load-bearing part. A pattern ending in `/` matches
    directories and never files -- that is precisely the fix the sync script's
    header records for its second incident, and a checker that ignores it
    reports `build-all.mk` and 30 book pages as lost. They are not: `build-*/`
    cannot match a file. Getting this wrong makes the gate cry wolf about the
    very files the last fix rescued.
    """
    dir_only = pattern.rstrip("*").endswith("/")
    pat = pattern.rstrip("*").rstrip("/")
    anchored = pat.startswith("/")
    pat = pat.lstrip("/")
    rx = re.escape(pat).replace(r"\*\*", ".*").replace(r"\*", "[^/]*")
    # A directory-only rule matches a PATH only when the matched run is
    # followed by more path -- i.e. it is a parent directory of this file.
    tail = r"/" if dir_only else r"(/|$)"
    if anchored:
        return re.match(rx + tail, path) is not None
    return re.search(r"(^|/)" + rx + tail, path) is not None


def is_dir_only(pattern):
    return pattern.rstrip("*").endswith("/")


def literal_prefix(pattern):
    """The leading, wildcard-free path an ANCHORED pattern names, or None.

    `/zephyr-workspace/**/build/***` -> `zephyr-workspace`. Used to ask whether
    the tree a carve-out protects is even present in this checkout.
    """
    if not pattern.startswith("/"):
        return None
    parts = []
    for seg in pattern.lstrip("/").split("/"):
        if "*" in seg or not seg:
            break
        parts.append(seg)
    return "/".join(parts) or None


def dir_verdict(rs, path, memo):
    """Index of the first rule that applies to everything UNDER `path`.

    A file's fate is decided almost entirely by its ancestor directories: a
    directory-only rule can never match at the file itself (it needs a `/`
    after the run), and a non-directory rule that matches an ancestor already
    settles the path. Evaluating the ancestors ONCE per directory rather than
    once per file is what keeps the widened sweep affordable -- `zephyr` alone
    contributes tens of thousands of files across a few thousand directories.

    Equivalence with `matches` is exact, not approximate: appending a component
    that cannot appear in any rule makes `path` an ancestor, so the directory
    rules see their required trailing `/` and nothing can match the sentinel.
    """
    if path in memo:
        return memo[path]
    probe = f"{path}/{SENTINEL}"
    idx = None
    for i, (_kind, pattern) in enumerate(rs):
        if matches(pattern, probe):
            idx = i
            break
    memo[path] = idx
    return idx


def file_verdict(rs, path, memo, file_rules):
    """First rule matching `path`, ancestors included. `None` = no rule."""
    parent = path.rsplit("/", 1)[0] if "/" in path else ""
    best = dir_verdict(rs, parent, memo) if parent else None
    for i, pattern, prefix in file_rules:
        if best is not None and i >= best:
            break
        if prefix and not path.startswith(prefix):
            continue
        if matches(pattern, path):
            return i
    return best


def file_rule_table(rs):
    """Rules that can match AT a file, cheapest-precheck first.

    Directory-only rules are absent by construction. Each survivor carries the
    literal prefix of an anchored pattern so the per-file work is a
    `str.startswith` for all but a handful of paths.
    """
    out = []
    for i, (_kind, pattern) in enumerate(rs):
        if is_dir_only(pattern):
            continue
        out.append((i, pattern, literal_prefix(pattern) or ""))
    return out


def git_env():
    """A repository-local git environment would redirect every call below.

    Same hazard `scripts/ci/submodule-pins-check.sh` clears for its fixtures
    (issue 0986): this gate runs from `just`, from CI and from the pre-push
    hook, and a hook is invoked with GIT_DIR already set.

    The list is git's own (`rev-parse --local-env-vars`), through the one
    shared helper, because the four names this function used to pop by hand
    were four of sixteen. Measured 2026-09-08: under the `explicit` hook shape
    the leaked `GIT_OBJECT_DIRECTORY` sent this gate's own fixture `git add`
    into the victim repository's object store — two loose objects, in the file
    that cites 0986 for doing the clearing. Popping SOME variables is not what
    satisfies the rule; asking git is.
    """
    return nros_clear_inherited_git_env(dict(os.environ))


def is_repo(path):
    # A submodule's `.git` is a FILE (a gitlink to `.git/modules/…`); a west
    # project's is a directory. Both are repositories whose index this gate
    # must read, and an `is_dir` test alone finds 14 of the 39 here.
    return os.path.exists(os.path.join(path, ".git"))


def source_trees(rs, root):
    """Every repository the mirror would copy, as (prefix, absolute path).

    The walk descends only where rsync would: a directory whose first matching
    rule is an `--exclude` is not mirrored, so nothing inside it can be lost
    and nothing inside it is visited. That prune is what makes this ~1 s rather
    than minutes -- the build output this repo carries dwarfs its source, and
    it is exactly what the rules already name.
    """
    memo = {}
    trees = [("", root)]
    stack = [""]
    while stack:
        rel = stack.pop()
        try:
            entries = os.scandir(os.path.join(root, rel) if rel else root)
        except OSError:
            continue
        with entries:
            for e in entries:
                if not e.is_dir(follow_symlinks=False) or e.name == ".git":
                    continue
                child = f"{rel}/{e.name}" if rel else e.name
                idx = dir_verdict(rs, child, memo)
                if idx is not None and rs[idx][0] == "exclude":
                    continue
                if is_repo(e.path):
                    trees.append((child, e.path))
                stack.append(child)
    return trees


def tracked(repo):
    """Paths `repo` has in its index, or None when git cannot answer."""
    try:
        out = subprocess.run(
            ["git", "-C", repo, "ls-files", "-z"],
            capture_output=True,
            text=True,
            env=git_env(),
        )
    except OSError:
        return None
    if out.returncode != 0:
        return None
    return [p for p in out.stdout.split("\0") if p]


# `/tmp/` is the repo's scratch directory and is gitignored (.gitignore:50).
# Ten files are tracked there anyway -- `collapse-*-case.sh` and two
# `migrate-*.py`, historical one-off repro and migration scripts committed
# before the convention settled. They are not build inputs, nothing in the
# box reads them, and re-including a directory the sync script deliberately
# drops would be the wrong fix.
#
# Listed by PREFIX rather than by name so a new scratch file does not fail
# this gate -- and called out here rather than silently skipped, because
# "tracked under a gitignored path" is itself worth someone's attention:
# a `git add` of one of these needs `-f`, so each was deliberate at the time.
ALLOWED_ABSENT = ("tmp/",)


def sweep(rs, trees):
    """(lost, examined, scratch) over every tree.

    `lost` is (path, pattern, tree); `scratch` counts the deliberate
    ALLOWED_ABSENT paths, reported so the OK line never implies they reached
    the mirror.
    """
    memo = {}
    file_rules = file_rule_table(rs)
    lost = []
    examined = scratch = 0
    for prefix, repo in trees:
        paths = tracked(repo)
        if paths is None:
            continue
        for p in paths:
            path = f"{prefix}/{p}" if prefix else p
            examined += 1
            idx = file_verdict(rs, path, memo, file_rules)
            if idx is None:
                continue
            kind, pattern = rs[idx]
            if kind != "exclude":
                continue
            if path.startswith(ALLOWED_ABSENT):
                scratch += 1
                continue
            lost.append((path, pattern, prefix or "<superproject>"))
    return lost, examined, scratch


def unverified(rs, root, trees):
    """Trees the sweep could NOT look at, by name.

    Two sources, both DERIVED rather than listed:

      * an anchored `--include` names a path. Those rules exist only to carve
        source out of a build-output exclusion, so one whose tree is absent is
        precisely a carve-out nobody checked. (A stale include -- one naming a
        path that no longer exists anywhere -- reports here too, which is
        information, not noise.)
      * `.gitmodules` declares a submodule and no repository is there.

    Deliberately NOT derived from the `--exclude` rules: `/third-party/make/`
    and `/third-party/ninja/` are documented as vestigial, so their absence is
    the expected state and reporting it would be the noise that teaches people
    to stop reading this line.
    """
    have = {prefix for prefix, _ in trees}
    out = []
    for kind, pattern in rs:
        if kind != "include":
            continue
        lp = literal_prefix(pattern)
        if lp and not os.path.exists(os.path.join(root, lp)):
            out.append(f"{lp} — carved out by --include '{pattern}', absent here")
    gitmodules = os.path.join(root, ".gitmodules")
    if os.path.exists(gitmodules):
        for m in re.finditer(r"^\s*path\s*=\s*(.+?)\s*$", open(gitmodules).read(), re.M):
            p = m.group(1)
            if p not in have:
                out.append(f"{p} — declared submodule, not checked out here")
    return out


def _fixture(tmp, with_reinclude):
    """A throwaway superproject with a nested repo shaped like the incident."""
    import shutil

    shutil.rmtree(tmp, ignore_errors=True)
    nested = os.path.join(tmp, "zephyr-workspace", "zephyr", "drivers", "i2c", "target")
    os.makedirs(nested)
    open(os.path.join(tmp, "top.txt"), "w").write("x\n")
    open(os.path.join(nested, "Kconfig"), "w").write("config I2C_TARGET\n")
    env = git_env()
    env.update(
        GIT_CONFIG_GLOBAL="/dev/null",
        GIT_CONFIG_SYSTEM="/dev/null",
    )
    for repo, add in ((tmp, "top.txt"), (os.path.join(tmp, "zephyr-workspace", "zephyr"), ".")):
        subprocess.run(["git", "init", "-q", repo], check=True, env=env)
        subprocess.run(["git", "-C", repo, "add", add], check=True, env=env)
    reinclude = "    --include '/zephyr-workspace/zephyr/**/target/***'\n"
    return (reinclude if with_reinclude else "") + "    --exclude 'target/'\n"


def mutation_self_test():
    """Re-introduce the defect and require a red — phase-395.

    The shape the gate was written for cannot be demonstrated on this checkout:
    an agent worktree and every CI lane carry no `zephyr-workspace` at all, so
    the sweep's happy path here compares one repository and proves nothing about
    the widening. The fixture supplies the missing tree, and the two mutations
    are the two states of the sync script on either side of the fifth incident.
    """
    import shutil
    import tempfile

    if not shutil.which("git"):
        return True
    tmp = tempfile.mkdtemp(prefix="nros-box-sync-gate-")
    try:
        for with_reinclude, want_lost in ((False, True), (True, False)):
            try:
                rule_text = _fixture(tmp, with_reinclude)
            except (OSError, subprocess.CalledProcessError) as exc:
                # LOUD, never `return True`: a selftest that skips itself when
                # its fixture will not build reports OK for a mutation it never
                # ran, which is the very shape this gate is about.
                print(f"self-test FAILED: could not build the fixture ({exc})", file=sys.stderr)
                return False
            rs = rules(rule_text)
            trees = source_trees(rs, tmp)
            if len(trees) != 2:
                print(
                    f"self-test FAILED: discovery found {len(trees)} tree(s), want 2 "
                    "(the superproject and the nested repo). A sweep that cannot "
                    "SEE the nested repo is the blind spot this gate was widened for.",
                    file=sys.stderr,
                )
                return False
            lost, _, _ = sweep(rs, trees)
            got_lost = bool(lost)
            if got_lost != want_lost:
                state = "WITH" if with_reinclude else "WITHOUT"
                print(
                    f"self-test FAILED: {state} the re-include the sweep reported "
                    f"{len(lost)} lost path(s), want {'some' if want_lost else 'none'}. "
                    "The mutation this gate exists to catch is not caught.",
                    file=sys.stderr,
                )
                return False
            if want_lost and not any(p.endswith("i2c/target/Kconfig") for p, _, _ in lost):
                print(
                    f"self-test FAILED: the loss was reported as {lost}, which does "
                    "not name the nested repo's file.",
                    file=sys.stderr,
                )
                return False
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return True


def self_test():
    """Run on the NORMAL path — a negative control nobody runs is a comment.

    The matcher cases are the two mistakes this checker has actually made, not
    invented ones. Its first version stripped the trailing `/` from every
    pattern, so a directory-only rule matched FILES: it reported `build-all.mk`
    and 30 book pages as lost, which are exactly the files the sync script's
    second fix rescued. A gate that cries wolf about the previous fix is worse
    than no gate.

    `dir_verdict` is the memoised restatement of `matches`, so it is checked
    against `matches` directly: a fast path that disagrees with the slow one is
    a gate reporting about a rule set nobody wrote.
    """
    cases = [
        # (pattern, path, should_match)
        # directory-only must NOT match a file with that prefix
        ("build-*/", "build-all.mk", False),
        ("build-*/", "book/src/internals/build-system.md", False),
        # ... but MUST match a file inside such a directory
        ("build-*/", "packages/cli/build-support/submodule_watch.rs", True),
        ("build/", "packages/cli/build/out.o", True),
        # anchored patterns bind at the root only
        ("/tmp/", "tmp/x.sh", True),
        ("/tmp/", "packages/tmp/x.sh", False),
        # a non-directory pattern still matches a file
        ("target-*/", "examples/a/target-zenoh/bin", True),
        # the fifth incident, at the shape the superproject sweep never saw
        ("target/", "zephyr-workspace/zephyr/drivers/i2c/target/Kconfig", True),
        ("/zephyr-workspace/zephyr/**/target/***",
         "zephyr-workspace/zephyr/drivers/i2c/target/Kconfig", True),
    ]
    bad = 0
    for pattern, path, want in cases:
        got = matches(pattern, path)
        if got != want:
            print(
                f"self-test FAILED: {pattern!r} vs {path!r} -> {got}, want {want}",
                file=sys.stderr,
            )
            bad += 1

    # The memoised decomposition must agree with the rule-by-rule scan it
    # replaces, on the real rule set.
    rs = rules()
    memo, ftab = {}, file_rule_table(rs)
    probes = [
        "packages/cli/build-support/submodule_watch.rs",
        "scripts/build/cargo.sh",
        "zephyr-workspace/zephyr/drivers/i2c/target/Kconfig",
        "zephyr-workspace/zephyr/scripts/build/dir_is_writeable.py",
        "third-party/px4/PX4-Autopilot/boards/modalai/voxl2/target/voxl-px4",
        "examples/workspaces/rust/Cargo.toml",
        "book/src/internals/build-system.md",
        "nros-patch.toml",
        "examples/a/target-zenoh/bin",
        "tmp/collapse-case.sh",
    ]
    for path in probes:
        slow = next((i for i, (_k, p) in enumerate(rs) if matches(p, path)), None)
        fast = file_verdict(rs, path, memo, ftab)
        if slow != fast:
            print(
                f"self-test FAILED: {path!r} -> rule {fast} memoised, {slow} scanned. "
                "The fast path and the rule order disagree.",
                file=sys.stderr,
            )
            bad += 1
    if bad:
        return False
    if not mutation_self_test():
        return False
    print(
        f"check-box-sync-covers-tracked-source self-test: OK "
        f"({len(cases)} shape(s), {len(probes)} memo probe(s), 2 mutation(s))"
    )
    return True


def main() -> int:
    if not self_test():
        return 1
    root = str(ROOT)
    rs = rules()
    trees = source_trees(rs, root)
    lost, examined, scratch = sweep(rs, trees)
    if lost:
        print(
            f"check-box-sync-covers-tracked-source: {len(lost)} TRACKED path(s) "
            f"would not reach the box mirror",
            file=sys.stderr,
        )
        for path, pattern, tree in lost[:20]:
            print(
                f"  {path}\n      excluded by  --exclude '{pattern}'  (tracked by {tree})",
                file=sys.stderr,
            )
        if len(lost) > 20:
            print(f"  ... and {len(lost) - 20} more", file=sys.stderr)
        print("", file=sys.stderr)
        print("  A build-output pattern has eaten tracked SOURCE — the class in", file=sys.stderr)
        print("  ros2-box-sync.sh's header, which has now happened five times.", file=sys.stderr)
        print("  Re-include the path AHEAD of the exclusion (rsync takes the", file=sys.stderr)
        print("  first matching rule), as `/packages/cli/build-support/***` is.", file=sys.stderr)
        return 1

    absent = unverified(rs, root, trees)
    nested = len(trees) - 1
    verdict = (
        f"check-box-sync-covers-tracked-source: OK ({examined} tracked path(s) across "
        f"{len(trees)} source tree(s): the superproject + {nested} nested repo(s); "
        f"{scratch} scratch path(s) under tmp/ deliberately not mirrored)"
    )
    if absent:
        strict = os.environ.get("NROS_BOX_SYNC_SWEEP_STRICT", "") not in ("", "0")
        for a in absent[:8]:
            print(f"NOT VERIFIED: {a}", file=sys.stderr)
        if len(absent) > 8:
            print(f"NOT VERIFIED: ... and {len(absent) - 8} more", file=sys.stderr)
        print(
            f"  {len(absent)} tree(s) are not in this checkout, so their source was "
            f"NOT swept.\n"
            f"  This is the normal state in CI and in an agent worktree. Provision "
            f"them\n"
            f"  (`nros setup`, `git submodule update --init <path>`) to turn the skip "
            f"into a\n"
            f"  verdict; NROS_BOX_SYNC_SWEEP_STRICT=1 makes it a failure on a lane "
            f"that\n"
            f"  claims to provide every tree.",
            file=sys.stderr,
        )
        if strict:
            return 1
        print(f"{verdict} — PARTIAL: {len(absent)} tree(s) NOT VERIFIED")
        return 0
    print(verdict)
    return 0


if __name__ == "__main__":
    sys.exit(main())
