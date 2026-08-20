#!/usr/bin/env bash
# Sample real CPU occupancy of a build. Issue 0726.
#
# Two mistakes this deliberately avoids, both of which produced wrong numbers:
#
#  * `pgrep -f <recipe name>` MATCHES THE SAMPLER ITSELF, because the pattern
#    appears in the sampler's own command line. A `while pgrep -f …` loop then
#    never exits and reports a build "running" long after it finished — one
#    measurement here was taken with no build alive at all. Track the build by
#    the PID captured at launch and use `kill -0`.
#  * /proc/loadavg's runnable field counts EVERY runnable process on the box,
#    including the sampler. Count build tools by name instead, so the number
#    means "compilers on CPU" rather than "things on CPU".
set -uo pipefail
pid="${1:?usage: $0 <build-pid> [out.tsv] [interval]}"
out="${2:-build-cpu.tsv}"
iv="${3:-2}"
# Exact names (`pgrep -x`), never a command-line match.
TOOLS='^(make|ninja|cargo|rustc|cc1|cc1plus|clang|gcc|ld|lld|ld.lld|cmake|west|python3|idf.py|xtensa-esp32-elf-gcc|riscv-none-elf-gcc|arm-none-eabi-gcc)$'
printf 'epoch\trunnable_build\talive_build\tloadavg_runnable\n' > "$out"
while kill -0 "$pid" 2>/dev/null; do
    # stat + comm for every process; R-state build tools vs all build tools.
    read -r r a < <(ps -eo stat,comm --no-headers | awk -v re="$TOOLS" '
        $2 ~ re { a++; if ($1 ~ /^R/) r++ } END { print r+0, a+0 }')
    l=$(awk '{split($4,x,"/"); print x[1]}' /proc/loadavg)
    printf '%s\t%s\t%s\t%s\n' "$(date +%s)" "$r" "$a" "$l" >> "$out"
    sleep "$iv"
done
