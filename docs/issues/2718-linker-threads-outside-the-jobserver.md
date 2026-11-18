---
id: 2718
title: "The host linker runs one thread per CPU outside the gate's token pool"
status: open
type: bug
area: [ci, testing, build]
severity: medium
found: 2026-11-19
related: [phase-559, 2625, 2683]
---

## What happens

A gate lane runs under one token pool (a make 4.4 jobserver,
`scripts/build/jobserver-pool.sh`), and every compile takes a token. The link
step does not. On x86_64 Linux, rustc links host artifacts with `rust-lld`,
which runs one thread per CPU by default. The pool never sees those threads.

Measured on host 139 (32 CPUs, a 32-token pool), `just check build` cold, with
the load sampler's above-pool report (`run-load-sampler.py --pool`, issue
2625):

```text
token pool: 1 gate test(s) ran more threads than the whole pool of 32, which
it cannot have granted: some child ran outside the jobserver (top commands at
those moments):
  build::check::cli-tests: peak 109; rust-lld 101, rustc 8
```

One gate ran 109 runnable threads against a pool of 32. 101 of them were the
linker's. `workspace-features`, which links a workspace's test binaries
(`cargo test --no-run --workspace`), reached 54 in an earlier run on the same
host. Neither reproduces alone (`workspace-features` peaked at 28 solo), since
it needs several links landing at once, in one gate or across gates.

The same gates are where the build tier's disk waits sit: in the run above,
`workspace-features` spent 19,105 thread-seconds waiting on disk and
`cli-tests` 13,185, against a few hundred running. Linking big debug test
binaries writes most of those bytes.

## Why it matters

The pool and the per-gate weights (issue 2625) bound what the lane starts. A
link outside both can put 3x the host's CPUs in the run queue at the moment
several gates link. That is the "load spike, then every gate slows at once"
shape issue 2683 records, and it lands on whichever gate's 2400 s clock is
running.

## Fix direction

Bound the linker by the same budget as everything else, in ONE place:

- `-C link-arg=-Wl,--threads=<n>` for host links made by the gate lanes, with
  `<n>` small (lld links are mostly I/O-bound, so a few threads lose little).
  It must reach every cargo root a gate builds, which is a RUSTFLAGS-carrier
  question (issue 0491: a path-free value, so no fingerprint churn).
- Or count links as jobs: hold one pool token per link. Fairer, but it needs
  a wrapper on the link step.

Measure the link wall time at the chosen `<n>` against the default, solo, as
`cli-tests`' knee was measured.

## Done when

Across several `NROS_GATE_WAITS=1 just check build` runs on a host whose CPU
count equals its pool, the above-pool report names no gate. At that point it
can become a failure rather than a report (issue 2625).
