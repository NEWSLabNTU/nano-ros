---
id: 1448
title: "`check-scaffold-builds` is RED on main for the `rust_project` variant —
  the build exits 0 and compiles none of the emitted source, so `just ci gate`
  cannot reach `test-unit` on any branch"
status: resolved
type: bug
area: tooling, ci
severity: high
found: 2026-09-22
related: [1446, 1439, 1435, 1409, 1058, 1310]
---

> **DUPLICATE of issue 1446 — archived 2026-09-22, not fixed.**
>
> 1446 and this issue were filed hours apart by two sessions against the same
> gate, the same variant and the same failure text. **1446 is the survivor**: it
> carries the root cause, which this issue did not reach — `own_artifacts()` in
> `scripts/check-scaffold-builds.sh` searches `$dir/target` (absent for this
> variant) and `$dir/build` at `-maxdepth 2`, and RFC-0098 D1 put the binary at
> `<leaf>/build/native/target/debug/`, four levels down. The build is fine; the
> locator cannot see it.
>
> Everything below that 1446 lacked has been MOVED INTO 1446 rather than lost:
> the blast radius (this red withdraws `check::api-parity`, `test-unit` and
> `test-lane-contracts` from `just ci gate` on every branch), the
> two-checkout / two-branch measurement table, the `severity: high`, and the
> "What this is NOT" list. The one thing deliberately NOT carried over is the
> "Where to start" section below, which points at the `nros new` Rust PROJECT
> template: that lead is wrong, and 1446 now records it as superseded so the
> next reader does not re-derive it.
>
> `status: resolved` because the archive gate is one-directional — an archived
> issue may not say `open` — and not because the gate is green. **The defect is
> still live; track it at 1446.**

## What happens

`just ci gate` fails at step 3 of 6 (`check::build`), on `check-scaffold-builds`:

```
===== FAIL (scaffold-builds, rc=1, 1444993ms) =====
self-test OK: an undeclared type in the emitted source is caught.
check-scaffold-builds: compiling 6 scaffold variant(s) outside the checkout
  rust_component: OK — 1 own artifact(s), e.g. target/debug/librust_component.rlib
  rust_project: FAIL — the build exited 0 but compiled none of the emitted source
  c_component: OK — 5 own artifact(s)
  c_project: OK — 4 own artifact(s)
  cpp_component: OK — 2 own artifact(s)
  cpp_project: OK — 4 own artifact(s)
check-scaffold-builds: FAILED — `nros new` emits something that does not compile.
```

One of six variants, and it is the one issue 1058's gate exists for: the build
reports success while compiling nothing the scaffold emitted, which is the
vacuous pass the predicate was written to refuse. The gate is doing its job;
what it is reporting is real.

## Why this is a MAIN-side red and not one branch's defect

MEASURED on two checkouts, two branches and two sessions, within an hour of
each other on 2026-09-22, both rebased onto `main` at `a0f00adad`:

| checkout | branch | result |
|---|---|---|
| `nano-ros-box2` | `fix/1437-c-cpp-granted-qos` | `rust_project: FAIL`, same wording, `check::build` 3 of 22 red |
| `nano-ros-box-box` | `feat/444-name-accessors` | `rust_project: FAIL`, same wording, `check::build` 2 of 22 red |

Neither branch touches `packages/cli`, the `nros new` templates, or anything a
scaffold compiles against except by ADDING inherent methods. Two unrelated
diffs producing one identical failure is the signature of a red that was
already on `main`.

## Why it matters more than one gate

`just ci gate` stops at the first failing step, so a red here **withdraws**
`check::api-parity`, `test-unit` and `test-lane-contracts` on every branch that
runs the lane — the lane CLAUDE.md tells every contributor to run before every
push. That is issue 1226's shape: a lane with no signal capacity, where the
next regression looks exactly like yesterday's failure. It is also why this
issue is filed rather than worked around: a contributor who reads
`CI GATE FAILED` here cannot tell their own defect from this one.

## What this is NOT

- **Not issue 1439.** That one is a RUNTIME failure of the scaffolded Rust
  entry — it builds, links, opens its session and exits without publishing.
  Here the build itself compiles none of the emitted source, so 1439's subject
  never runs.
- **Not a disk-space or load artifact.** The two runs above were on different
  checkouts with different build state, and every other variant in the same
  invocation passed.
- **Not the flake beside it.** `a_stalled_timer_reports_a_timer_overrun_violation`
  also failed in the same sweep and passes solo (0.13 s); that one is the
  known in-sweep timing flake, not this.

## Where to start

`packages/cli`'s `nros new` Rust PROJECT template, and what changed under it
this week: #1150 (issue 1409, the entry template emitted `std` for a board that
has none), #1155 and `a0f00adad` (issue 1435, the template rendered a
`BoardEntry::run` four board keys have no impl for). The component variant
passes and the project variant does not, so the divergence is in what the
project template emits or in the workspace shape it emits it into — compare
against issue 1310, which is the same "a `--lang rust` scaffold is never
built" surface one level up.

The gate keeps its own full build log; the path is printed in the failure
(`/tmp/nros-scaffold-builds.*/build.log`) and is the first thing to read.

## What would close it

`just check scaffold-builds` green on `main` with all six variants reporting
their own artifacts, and — because this red rode in behind a lane nobody could
read — a note in the closing PR saying which of the three recent template
changes introduced it.
