#!/usr/bin/env bash
# Per-LEAF occupancy of a build, for critical-path analysis. Issue 0805.
#
# `sample-build-lineage.sh` answers "how much of this build is on CPU" and
# `sample-build-wchan.sh` answers "what is it blocked on". Neither answers "which
# leaf, and how many at once" — and without that, a component's size cannot be
# turned into its effect on the WALL. Issue 0805 removed ~750 s of archive
# post-processing and the wall moved 30 s, precisely because the work was
# overlapped; only a per-leaf span map shows that in advance.
#
# Attribution is by LINEAGE then by PATH: walk the build's descendants (never a
# global name match — see sample-build-lineage.sh for what that cost), then map
# each process to a leaf by finding an `examples/<...>/<leaf>/build-<x>` segment
# in its cwd or command line. Processes that match no leaf are counted as
# `(driver)`, which is the make/shell scaffolding and should be most of them.
#
# Output: one row per (sample, leaf) with how many processes that leaf had, and
# how many of them were running. Spans and concurrency are derived from that.
set -uo pipefail
pid="${1:?usage: $0 <build-pid> [out.tsv] [interval]}"
out="${2:-build-leaves.tsv}"
iv="${3:-1}"

printf 'epoch\tleaf\tprocs\trunning\n' > "$out"

descendants() {
    awk -v root="$1" '
        FILENAME ~ /\/stat$/ {
            n = split($0, f, ")"); split(f[n], g, " ")
            pid = $1; ppid = g[2]
            child[ppid] = child[ppid] " " pid
            state[pid] = g[1]
        }
        END {
            queue = root; seen = ""
            while (queue != "") {
                split(queue, q, " "); queue = ""
                for (i in q) {
                    p = q[i]; if (p == "" || index(seen, " " p " ")) continue
                    seen = seen " " p " "
                    if (p != root) print p, state[p]
                    if (p in child) queue = queue " " child[p]
                }
            }
        }
    ' /proc/[0-9]*/stat 2>/dev/null
}

while kill -0 "$pid" 2>/dev/null; do
    now=$(date +%s)
    descendants "$pid" | while read -r p st; do
        # cwd first: it is the cheapest reliable locator for a per-leaf build.
        loc=$(readlink "/proc/$p/cwd" 2>/dev/null)
        case "$loc" in
            *"/examples/"*) ;;
            *) loc=$(tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null | grep -oE '/examples/[^ ]*/build-[a-z0-9]+' | head -1) ;;
        esac
        leaf=$(printf '%s' "$loc" | grep -oE 'examples/[^/]+/[^/]+/[^/]+/build-[a-z0-9]+' | head -1)
        [ -z "$leaf" ] && leaf="(driver)"
        r=0; case "$st" in R*) r=1;; esac
        printf '%s\t%s\t1\t%s\n' "$now" "$leaf" "$r"
    done >> "$out"
    sleep "$iv"
done
