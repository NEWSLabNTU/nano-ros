#!/usr/bin/env bash
# Occupancy of a build, counting ONLY that build's own processes. Issue 0726.
#
# Supersedes `sample-build-cpu.sh`, which counted build tools by `comm` across
# the whole box. That swept up an unrelated colcon2deb/Autoware container build
# running on the same host — 22 processes — and produced a set of numbers
# (cmake dominating, 22.6% I/O waits) that described someone else's build.
#
# Two rules, both learned the hard way in this phase:
#
#   * Track the build by the PID captured at launch and use `kill -0`. NEVER
#     `pgrep -f <recipe name>`: that pattern appears in the sampler's own
#     command line, so the sampler matches itself, the wait loop never exits,
#     and it reports a build "running" hours after it finished.
#   * Attribute processes by LINEAGE, not by name. A descendant walk answers
#     "what is THIS build doing"; a name match answers "what on this machine
#     happens to be called cargo".
#
# Output is one row per sample: how many of the build's own descendants exist,
# how many are on CPU, and how many are blocked in each state. `loadavg` is
# recorded alongside purely so the two can be COMPARED — a large gap between
# `runnable` and `load_runnable` means something else is using the machine, and
# the run should be discarded rather than explained.
set -uo pipefail
pid="${1:?usage: $0 <build-pid> [out.tsv] [interval]}"
out="${2:-build-lineage.tsv}"
iv="${3:-2}"

printf 'epoch\talive\trunnable\tsleeping\tdisk_wait\tload_runnable\n' > "$out"

# Descendants of $pid, via one pass over /proc's ppid links. Iterative rather
# than recursive so a deep make/ninja/cargo tree cannot blow the stack.
descendants() {
    local root="$1"
    awk -v root="$root" '
        FILENAME ~ /\/stat$/ {
            # comm can contain spaces and parens; ppid is the field after the
            # last ")" plus one.
            n = split($0, f, ")")
            rest = f[n]
            split(rest, g, " ")
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
    read -r alive run slp dw < <(descendants "$pid" | awk '
        { a++; if ($2 ~ /^R/) r++; else if ($2 ~ /^D/) d++; else s++ }
        END { print a+0, r+0, s+0, d+0 }')
    load=$(awk '{split($4,x,"/"); print x[1]}' /proc/loadavg)
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$(date +%s)" "$alive" "$run" "$slp" "$dw" "$load" >> "$out"
    sleep "$iv"
done
