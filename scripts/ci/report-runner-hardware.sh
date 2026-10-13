#!/usr/bin/env bash
# phase-489 W5 — print the CPU a CI job landed on. Evidence only: it never
# fails a job, and nothing reads its output but a human comparing run times.
set -u
echo "--- runner hardware ---"
if command -v lscpu >/dev/null 2>&1; then
    lscpu | grep -E '^(Model name|CPU\(s\)|Thread\(s\) per core|CPU MHz|CPU max MHz|BogoMIPS|L3 cache)' || true
else
    grep -m1 'model name' /proc/cpuinfo || true
    echo "CPU(s): $(nproc 2>/dev/null || echo '?')"
fi
echo "cpu MHz (per core): $(awk -F: '/cpu MHz/ {printf "%d ", $2}' /proc/cpuinfo 2>/dev/null)"
exit 0
