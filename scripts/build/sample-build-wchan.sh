#!/usr/bin/env bash
# What is the build WAITING ON? Issue 0726, follow-on to sample-build-lineage.sh.
#
# That sampler established the build's own processes are 92.6% idle: 36 alive,
# 0.1 runnable. This one answers what the other ~36 are doing, by sampling the
# kernel function each is blocked in (`/proc/<pid>/wchan`) — restricted to the
# SAME LINEAGE, because the previous wchan attempt matched by process name and
# its numbers described a concurrent Autoware build.
#
# The distinction that matters, and the reason `has_child` is recorded:
#
#   A `make` or `sh` sitting in `do_wait` with children is a SUPERVISOR. It is
#   correctly blocked on work it already dispatched, and counting it as "the
#   build is stalled" is double-counting the thing it is waiting for. A LEAF
#   (no children) blocked in futex/pipe_read/io is different — that is work
#   that cannot proceed.
#
# So every row carries: comm, state, wchan, and whether the process has
# children. Aggregate leaves and supervisors separately or the answer is noise.
set -uo pipefail
pid="${1:?usage: $0 <build-pid> [out.tsv] [interval]}"
out="${2:-build-wchan.tsv}"
iv="${3:-3}"

printf 'epoch\tcomm\tstate\twchan\thas_child\n' > "$out"

while kill -0 "$pid" 2>/dev/null; do
    now=$(date +%s)
    # Build the ppid->children map and the descendant set in one awk pass, then
    # emit per-descendant rows. Reading /proc twice would race a build that
    # forks this fast.
    mapfile -t kids < <(awk '
        FILENAME ~ /\/stat$/ {
            n = split($0, f, ")"); split(f[n], g, " ")
            print $1, g[2]
        }' /proc/[0-9]*/stat 2>/dev/null)
    declare -A CH=(); declare -A PARENT=()
    for row in "${kids[@]}"; do
        set -- $row
        PARENT[$1]=$2; CH[$2]=1
    done
    # descendant closure from $pid
    queue=("$pid"); seen=""
    while [ ${#queue[@]} -gt 0 ]; do
        cur="${queue[0]}"; queue=("${queue[@]:1}")
        case " $seen " in *" $cur "*) continue;; esac
        seen="$seen $cur"
        for p in "${!PARENT[@]}"; do
            [ "${PARENT[$p]}" = "$cur" ] && queue+=("$p")
        done
    done
    for p in $seen; do
        [ "$p" = "$pid" ] && continue
        c=$(cat "/proc/$p/comm" 2>/dev/null) || continue
        st=$(awk '{n=split($0,f,")"); split(f[n],g," "); print g[1]}' "/proc/$p/stat" 2>/dev/null)
        w=$(cat "/proc/$p/wchan" 2>/dev/null); [ -n "$w" ] || w='(running)'
        hc=$([ -n "${CH[$p]:-}" ] && echo yes || echo no)
        printf '%s\t%s\t%s\t%s\t%s\n' "$now" "$c" "$st" "$w" "$hc" >> "$out"
    done
    unset CH PARENT
    sleep "$iv"
done
