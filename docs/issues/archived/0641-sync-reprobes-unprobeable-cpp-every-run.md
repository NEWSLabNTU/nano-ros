---
id: 641
title: "`nros sync` re-ran a CMake metadata probe that cannot succeed, on every run — a Rust package read as C++, and no negative cache on the batch path"
status: resolved
type: performance
severity: medium
area: cli, build
related: [issue-0288, issue-0522, issue-0562, phase-308, phase-313]
---

## Symptom

`nros sync` was slow on two workspaces and fast on the other twenty:

```
examples/workspaces/mixed        1.24 s
examples/workspaces/features     1.21 s
everything else             0.18–0.29 s
```

`regenerate-bindings.sh` runs `nros sync` over 22 workspace roots at the head of
every fixture build, so the cost is paid on every build.

## What it was doing

`strace -c` on one WARM `mixed` sync: **1167 `execve`s, and 88 % of wall-clock in
`wait4`** — 78 `cmake`, 57 `/bin/sh`, and ~40 `git`. A sync that should read and
write files was configuring and building a CMake project, Corrosion and all:

```
build/nros-metadata/metadata-probe-cmake/build/_deps/corrosion-build
```

That is the phase-308 C/C++ metadata probe. It is supposed to run only when a
sidecar is missing or its sources changed. Both workspaces ran it every time,
and in both cases it **could not succeed**.

### `mixed` — a Rust package routed to the C++ probe

```
fatal error: rust_heartbeat_pkg/Heartbeat.hpp: No such file or directory
```

`rust_heartbeat_pkg/CMakeLists.txt` says `LANGUAGE RUST`. `infer_cmake_language`
knew `c`, `cpp` and `cxx`; `rust` fell through to the `_` arm, which guesses
from the class shape — `rust_heartbeat_pkg::Heartbeat` contains `::`, so **Cpp**.
The function's own doc comment says *"`LANGUAGE` is authoritative when present"*,
and for `RUST` it was not.

The silent fallback is what hid it: the declaration said one thing, the
inference did another, and nothing printed.

### `features` — 17 components whose probe project fails to configure

A different cause with the same shape: the batch's CMake configure fails, so all
17 components are reported unprobeable — and all 17 are retried next sync.

## The general defect

**The C/C++ batch path had no negative cache.** The Rust branch has had one
since issue 0288 (`is_known_unprobeable` / `mark_unprobeable`, keyed on a source
digest), so a Rust probe that fails is not retried until its sources change. The
batch path added to `cpp_batch` and ran, unconditionally, for ever.

So an unprobeable C/C++ component costs a full CMake configure + build on every
`nros sync`, with no possible progress — 22 times per fixture build.

## Fix (2026-08-16)

**1. `LANGUAGE RUST` is honoured** (`rust` and `rs`), and an UNRECOGNISED value
now falls back *loudly* to the class heuristic instead of silently.

**2. The batch path gets the negative cache**, per component:

* skipped when the marker matches, and the skip is REPORTED by name — 17 lines
  for `features`, not silence;
* written on a per-component build failure AND on a whole-project configure
  failure, since every component in the batch shares the project that failed.

**3. The marker key is `source_digest + NROS_CLI_SOURCE_STAMP`,** not the digest
alone. `source_digest` mixes only `CARGO_PKG_VERSION`, which moves on a release
bump and not on a fix — fine for a POSITIVE cache, where a stale sidecar is
caught by the coverage gate, and wrong for a negative one, where a stale marker
would **hide a probe someone had just repaired**. The stamp is baked by
`build.rs` from the CLI's own sources, so it changes exactly when the thing that
might fix the probe changes, and costs nothing at run time.

## Measured

| | before | after |
| --- | --- | --- |
| `sync examples/workspaces/mixed` | 1.24 s | **0.26 s** |
| `sync examples/workspaces/features` | 1.21 s | **0.27 s** |
| 22-workspace sync loop | 7.0 s | **5.1 s** |
| `regenerate-bindings.sh` (whole) | 12.8 s | **10.9 s** |

`mixed` also stopped lying about its coverage: `7 already current` instead of
`6 already current` plus a permanently failing probe.

A CLI rebuild invalidates the markers by design, so the first
`regenerate-bindings.sh` after one costs 24.2 s — every previously-failing probe
is retried exactly once, then cached. That is the point of keying on the stamp.

### Invalidation verified, not assumed

| step | probes attempted |
| --- | --- |
| cached | 0 |
| append a line to one component's `.c` | **1** (that component, not all 17) |
| revert | 0 |

Per-component, keyed on that package's own sources.

## Not fixed here

**Why `features`' probe project fails to configure.** 17 components have no
metadata sidecar and silently fall back to the SystemModel bound. This issue
makes that cheap instead of expensive; it does not make it correct. The
configure log also shows `Corrosion not provisioned — fetching v0.6.1 from git`
inside the probe, which is a second cost worth its own look.
