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
            r"\$output_dir", r"\$BUILD_DIR", r"\$build_", r"\$log_dir")

FILES = subprocess.run(
    ["git", "ls-files", "scripts", "just", "justfile"],
    capture_output=True, text=True).stdout.split()

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
