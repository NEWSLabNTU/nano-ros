#!/usr/bin/env bash
# Forbid `find` scans for files that git already tracks.
#
# Measured on a built tree, with a fixture build running concurrently:
#
#   find examples <pruned> -path '*/src/*/package.xml'   232 results   7m36s
#   git ls-files 'examples/*/src/*/package.xml'          232 results   0.8s
#
# 570x, identical output, and the `find` burned 0% CPU the whole time — it was
# never compute-bound, it was starved on I/O walking build trees.
# `scripts/regenerate-bindings.sh` ran that scan three times per invocation,
# which was most of a two-hour fixture build.
#
# The trap is that PRUNING LOOKS LIKE IT FIXES THIS AND DOES NOT. `find` must
# stat a directory to decide whether to prune it, so `-prune` cuts the descent
# but not the walk. The comment that used to sit above that scan asserted
# pruning made it fast; it was wrong, and being wrong in a confident comment is
# why nobody re-measured for a long time.
#
# So the rule is not "avoid find". It is:
#
#   Never `find` for a file git tracks. Use `git ls-files`.
#
# Scans for UNTRACKED artifacts are legitimate and stay: *.o, *.su, built ELFs,
# `target/`/`generated/` dirs being deleted by a clean recipe. git cannot see
# those. They must be scoped to a build directory rather than to `examples/` or
# `packages/`, which is what the check below enforces.
set -uo pipefail
cd "$(dirname "$0")/.."

python3 - "$@" <<'PY'
import re, subprocess, sys

# File kinds that are always tracked. `generated`/`target`/`build*` are
# deliberately absent — those are the untracked trees a clean recipe must walk.
TRACKED = r"package\.xml|Cargo\.toml|Cargo\.lock|\*\.rs|README\.md|\*\.msg|\*\.srv|\*\.action|\*\.launch\.xml|system\.toml"

# Roots whose subtrees contain build output, so a `find` rooted here pays the
# walk. The optional quote matters: the two stale-CLI guards were written
# `find "$root/packages/cli"` and slipped past an unquoted-only pattern.
# Any root INSIDE the repo. Originally an explicit list of literal dirs, which
# missed `find "$dir"` in generate-rust-incremental.sh — a per-package scan that
# ran 230+ times per invocation, the worst possible shape. Matching any variable
# root is the safer default: an out-of-repo root (a ROS install prefix) is the
# rare case, and it is excluded below by name rather than by omission.
ROOT = r'"?(?:examples|packages|\$\{?\w+)'

# Roots where git has NO index entry to consult, so `find` is the only option:
# either outside the repo entirely (a ROS install prefix), or a build/staging
# directory whose contents are copies rather than tracked files.
#
# `$staged` is the one that matters and the one this check first got wrong: the
# idf/compile-check fixtures COPY an example into `build/<id>/` and rewrite the
# path deps in the copy. Those `Cargo.toml`s are untracked by construction, so
# flagging them was the gate telling someone to use a tool that cannot see the
# files. A gate that demands an impossible fix gets disabled, so this list is
# part of the rule, not an escape hatch.
NO_INDEX = (r"\$prefix", r"\$ROS", r"/opt/ros", r"\$staged", r"\$out",
            r"\$output_dir", r"\$BUILD_DIR", r"\$build_", r"\$log_dir",
            # issue 0844 — derived from `$ROS_SHARE` in
            # rosidl-codegen/scripts/check_parser_failures.sh: a ROS install
            # prefix, so the .msg files are outside the repo and the index
            # cannot see them at all. Surfaced when the scan widened past
            # `scripts/`.
            r"\$MSG_DIR")

# issue 0844 — every tracked shell script, not just `scripts/`. The 37-minute
# `grep -r` over 9.2 GB of gitignored SDK lived in
# `packages/testing/nros-tests/tests/core_only_predicate.sh`, which this gate
# never opened: the scope was `scripts just justfile`, so the whole test-script
# tree — 46 files — was unscanned. A gate that policed one directory while
# stating a repo-wide rule is the same shape as the rule/pattern mismatches
# recorded above, one level out.
FILES = subprocess.run(
    ["git", "ls-files", "scripts", "just", "justfile", "*.sh"],
    capture_output=True, text=True).stdout.split()
FILES = [f for f in FILES
         if not f.startswith("third-party/") and "/third-party/" not in f]

bad = []
for f in FILES:
    if not (f.endswith(".sh") or f.endswith(".just") or f == "justfile"):
        continue
    if f.endswith("check-no-tracked-file-find.sh"):
        continue
    try:
        lines = open(f).read().split("\n")
    except OSError:
        continue
    # Join backslash continuations, keeping the FIRST physical line number.
    # Nearly every real find in this repo spans lines, so a line-wise regex
    # silently passes them — the first version of this gate did exactly that
    # and reported OK on a tree that still had one.
    i = 0
    while i < len(lines):
        start = i
        buf = lines[i]
        while buf.rstrip().endswith("\\") and i + 1 < len(lines):
            i += 1
            buf = buf.rstrip()[:-1] + " " + lines[i].strip()
        stripped = buf.lstrip()
        if not stripped.startswith("#"):
            # Stop at a pipe: `find ... | xargs grep -l package.xml` is a filter
            # on find's OUTPUT, not a scan FOR that name.
            head = buf.split("|")[0]
            m = re.search(r"find\s+" + ROOT + r".*?(?:-name|-path)\s+'?\"?(" + TRACKED + ")", head)
            if m and not any(re.search(o, head) for o in NO_INDEX):
                bad.append(f"  {f}:{start+1}: find searches for {m.group(1)}")

            # Recursive grep is the same defect wearing a different name: it
            # walks every build tree under the root to rediscover files the
            # index already names. `git grep -- <pathspec>` is the fix, and it
            # takes the same regex, so the conversion is mechanical.
            g = re.search(r"(?<!git )\bgrep\s+-[a-zA-Z]*[rR]", head)
            if g and not any(re.search(o, head) for o in NO_INDEX):
                bad.append(f"  {f}:{start+1}: recursive grep — use `git grep -- <pathspec>`")
        i += 1

# --- python arm (issue 0721) -------------------------------------------------
# The shell side of this repo is clean because this gate has policed it. The
# python side was never read — the filter above accepts only .sh/.just/justfile,
# though this gate is itself python scanning shell — so it accumulated 21 walk
# sites, two of which never finished: `rglob("Cargo.toml")` over the 828 GB
# examples/ tree ran >300 s against 0.002 s for the same 347 paths from the
# index.
#
# Python hides the root behind a variable far more often than shell does, so
# rather than guess which roots are big, EVERY recursive walk in a scanned file
# must either go through the index or carry a marker saying why it cannot:
#
#     # walk-ok: <reason>
#
# anywhere in the contiguous comment block above the line, or trailing on it.
# Legitimate reasons are real and common here — build dirs, staging copies, a
# tree being deleted, an untracked submodule — so this is part of the rule, not
# an escape hatch. A gate that demands an impossible fix gets disabled.
# issue 0726 — `recursive=True` is the fourth spelling, and it slipped the three
# above for two years' worth of gates. The rule this file states is "EVERY
# recursive walk", but the pattern required the `**` to be a LITERAL right after
# `.glob(`. Both offenders wrote it with the pattern in a variable and the path
# computed:
#
#     for pat in ("examples/**/Cargo.toml", ...):
#         glob.glob(os.path.join(ROOT, pat), recursive=True)
#
# invisible to `\.glob\(\s*["']\*\*`. `check-deploy-board-resolves` was measured
# at 23-24 MINUTES inside `check-fast -P32` (1.86 s through the index), and
# `check-site-config` had the same shape. Matching `recursive=True` catches the
# spelling regardless of where the pattern or the root came from — which is the
# property the other three alternatives lack.
PY_WALK = re.compile(r"\.rglob\(|\.glob\(\s*[\"']\*\*|\bos\.walk\(|recursive\s*=\s*True")
PY_ALLOW = re.compile(r"walk-ok:")

PY_FILES = [f for f in FILES if f.endswith(".py")]
for f in PY_FILES:
    if f.endswith("check-no-tracked-file-find.py"):
        continue
    try:
        lines = open(f).read().split("\n")
    except OSError:
        continue
    # Triple-quoted blocks are PROSE, not code. `scripts/lib/tracked.py` — the
    # helper this rule exists to send people to — documents the antipattern by
    # showing it, so the gate flagged its own remedy and `just check` went red
    # on a file nobody could fix without deleting the explanation. A docstring
    # that quotes forbidden code is the normal way to explain why it is
    # forbidden; the rule is about what the interpreter RUNS.
    in_doc = None
    for n, line in enumerate(lines, 1):
        if in_doc is not None:
            if in_doc in line:
                in_doc = None
            continue
        opened = False
        for delim in ('"""', "'''"):
            # Odd count = the block is still open when the line ends. An even
            # count is a complete one-line string, which is also not code.
            if line.count(delim) % 2 == 1:
                in_doc = delim
                opened = True
                break
        if opened or ('"""' in line or "'''" in line):
            continue
        stripped = line.lstrip()
        if stripped.startswith("#"):
            continue
        if not PY_WALK.search(line):
            continue
        if PY_ALLOW.search(line):
            continue
        # Contiguous comment block immediately above, same idiom as
        # check-no-std-stdio's exemptions: a reason worth giving runs to
        # several lines, and requiring it on the last one invites one-liners.
        j, allowed = n - 2, False
        while j >= 0 and lines[j].strip().startswith("#"):
            if PY_ALLOW.search(lines[j]):
                allowed = True
                break
            j -= 1
        if not allowed:
            bad.append(f"  {f}:{n}: recursive walk — use `git ls-files`, "
                       f"or mark it `# walk-ok: <reason>`")

if bad:
    print("FAIL: filesystem walk used to locate git-tracked files:")
    print("\n".join(bad))
    print()
    print("  Use `git ls-files` / `git grep` — an index lookup, not a walk.")
    print("  Measured: 7m36s -> 0.8s for the same 232 paths. Pruning does NOT")
    print("  fix it; find still stats every directory it considers pruning.")
    print()
    print("  Scanning for UNTRACKED artifacts (*.o, built ELFs, target/ dirs)")
    print("  is fine — scope it to a build dir, not to examples/ or packages/.")
    sys.exit(1)

print("no-tracked-file-find OK — tracked-file discovery goes through git ls-files.")
PY
