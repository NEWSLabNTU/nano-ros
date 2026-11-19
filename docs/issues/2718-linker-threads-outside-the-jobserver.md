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

## Deferred (manager ruling, 2026-11-19): the carrier, costed

Not fixed now. On the hosts where today's gate timeouts were measured, the
load came from other tenants' processes (issue 2683's waits tables), not
from link threads. Both carriers below cost more than the measured gain. If
`rust-lld` shows up as a top command in a gate waits table, start here.

What any carrier must do: cap the threads of the host link (`rust-lld`,
reached through `cc -fuse-ld=lld`) for every cargo root a gate builds. Those
roots are the root workspace, `packages/cli`, the generated probe and lint
roots, and the leaves. It must do this without changing a fingerprint
between a gate build and a developer build of the same tree (issue 0491).

**Option A: `-C link-arg=-Wl,--threads=<n>` in `.cargo/config.toml`.**

- It needs a host-target table: `[target.'cfg(all(target_arch = "x86_64",
  target_os = "linux"))'] rustflags`. Cargo ignores `[build] rustflags` once
  a matching `target.*.rustflags` is set, so the table must repeat
  `-C symbol-mangling-version=v0` from `[build]`. Two lists that must agree
  need a gate of their own.
- It applies checkout-wide, not to gates only: the config's walk-up reaches
  every root under the checkout. Every host artifact rebuilds once, because
  the rustflags change.
- It assumes lld. GNU ld (bfd, 2.38 here) has no `--threads` (`ld.bfd --help`
  lists none). A user who overrides the host linker therefore gets a link
  error from a flag they never wrote. No config-level condition can ask which
  linker a build will use.

**Option B: count a link as a job.** A `linker =` wrapper takes one pool
token for the link's duration (or `n` tokens for `--threads=n`) through the
jobserver's fifo, then runs the real linker.

- It is fairer: the pool then bounds links like compiles.
- It needs the same host-target config entry as A (`linker =` instead of
  `rustflags`), so it is checkout-wide too. A link outside a pool must pass
  straight through.
- It is a new program on every host link.

**Rejected: an environment carrier (`RUSTFLAGS`,
`CARGO_TARGET_<triple>_RUSTFLAGS`).** It reaches the gates only. But it
REPLACES the config's `rustflags` rather than adding to them, so it drops
the v0 mangling. Every gate build would then fingerprint differently from a
developer build of the same tree, splitting the cache between them, which is
issue 0491's class.

**Measure first, whichever lands:** the solo link wall time of the largest
gate links (`cli-tests`, `workspace-features`) at `--threads=4` against the
default, as `cli-tests`' knee was measured.
