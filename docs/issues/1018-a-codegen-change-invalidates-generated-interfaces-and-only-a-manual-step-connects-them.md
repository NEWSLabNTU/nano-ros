---
id: 1018
title: "A codegen change invalidates every consumer's generated interfaces, and only a manual `setup-cli` connects them"
status: open
area: build, codegen
severity: medium
related: [0963, 0965, phase-403]
---

# The staleness check is right, and it is the only thing holding the chain together

## The dependency

```
rosidl-codegen sources  ->  the in-tree `nros` CLI  ->  every consumer's generated interfaces
```

Neither arrow is automatic. `just setup-cli` rebuilds the CLI by hand, and a
consumer's interfaces regenerate only when its own build runs. The single thing
connecting them is `NanoRosCodegenCore.cmake`'s staleness check:

```
Error: in-tree nros CLI is STALE -- its sources changed since it was built
```

## What it costs

Observed three times in one afternoon of phase-403 work, each time from an
ordinary action:

1. editing `rosidl-codegen` (the whole point of the phase),
2. moving the submodule pin forward to a merged `main`,
3. editing it again for the follow-up wave.

Each stop needs the same manual recovery -- rebuild the CLI, rebuild the image
-- and the second case is the interesting one: nothing in the consumer's tree
changed at all. Taking a NEWER upstream is enough to stale the CLI, so a user
who did nothing but update a pin is told their CLI is out of date.

## Why the check must stay

It is doing real work, and it caught a case that would have been silent. The
C emitter's sequence-of-strings dimensions were transposed in all three emission
sites (`string[]` produced `char data[256][64]` instead of `[64][256]`); the fix
lives in codegen. Without the staleness check the consumer would have
regenerated with the OLD binary and reported success, with the tree claiming the
fix was in. Same shape for phase-403's derived bounds: a stale CLI emits the
previous rule's numbers and nothing says so.

So this is not "delete the check". It is that the check is the ONLY thing
holding the chain, and it holds it by stopping the build rather than by fixing
it.

## What would resolve it

Options, none chosen:

1. **Make the CLI a build dependency of codegen**, so a consumer's build rebuilds
   it rather than refusing. Cost: every consumer build can now compile a Rust
   binary, which on the Zephyr lane is a surprise, and it hides the cost of a
   codegen change rather than naming it.
2. **Stamp the generated output with the CLI's source hash** and regenerate when
   it differs, which is what the fixture stamps already do elsewhere in this
   tree. The consumer then rebuilds interfaces without rebuilding the CLI,
   and the CLI rebuild stays explicit.
3. **Keep the refusal and make recovery one step** -- the message names
   `just setup-cli`, so a `--fix` affordance or a recipe that does both would
   turn three manual recoveries into three keystrokes.

(2) matches how this repo already handles derived state elsewhere, and it is
the only one that distinguishes "the CLI is stale" from "your generated code is
stale", which are different problems with different costs.

## Adjacent

The same shape as issue 0963: the build knows a fact -- here, that the CLI
predates its sources -- and can only say so by stopping. 0963 is about numbers
that are computed and never read; this is about a dependency that is detected
and never acted on.

## NARROWED 2026-09-05 (phase-424) — the right arrow was broken, not just blunt

The claim above that "the check is the ONLY thing holding the chain" is **false,
and measuring it found a worse bug than the one reported.**

### What already held, measured

`cmake/NanoRosGenerateInterfaces.cmake:405` carries
`DEPENDS … "${_NANO_ROS_CODEGEN_TOOL}"`, and it has since 2026-05-23. Built a
minimal consumer against that generator and touched the CLI binary: the codegen
command re-ran (`ninja -t query` lists the binary as an input), and — because
CMake gives custom commands `restat = 1` and codegen writes only-if-changed —
**nothing downstream rebuilt and the edge settled on the next build**. So the
over-firing cost of a `just setup-cli` on the BUILD-time lane is one codegen
run per package, not a cascade. My first hypothesis, that the command would be
permanently dirty, was refuted by running it.

### What did not hold

A CONFIGURE-time emitter cannot express that edge. `execute_process()` has
already run by the time ninja decides anything, so its freshness reduces to
*does a configure happen at all*. Four sites emit at configure time:

| site | verb | registered the tool for reconfigure |
| --- | --- | --- |
| `cmake/NanoRosEntry.cmake` | `codegen entry` | yes — issue #182, inline, in its own function |
| `zephyr/cmake/nros_generate_interfaces.cmake` | `codegen` | **no** |
| `zephyr/cmake/nros_system_generate.cmake` | `codegen-system` | **no** |
| `integrations/nano-ros/CMakeLists.txt` (ESP-IDF) | `codegen-system` | **no**, and it fails SOFT |

The Zephyr interfaces generator even carries the right predicate already — an
`IS_NEWER_THAN` loop at `:284` that names the tool — and it is dead on an
incremental build, because nothing makes `build.ninja` stale when the tool
moves. Measured: `zephyr-workspace/build-rust-talker-zenoh`'s `RERUN_CMAKE`
edge lists **3592 inputs, none under `packages/cli`**.

So a Zephyr image inherited codegen freshness only if it ALSO happened to call
`nano_ros_entry()`. Grep: **no `examples/zephyr/**/CMakeLists.txt` calls it.**
Those images generate their interfaces at configure time and keep museum
generated code after a `nros` rebuild — silently, which is the failure this
issue argued the stale check exists to prevent, arriving through the door
nobody was watching. #182 fixed the site; this is the class.

### The fix

`nros_codegen_tool_reconfigure(<tool>)` in `cmake/NanoRosCodegenCore.cmake` —
one deduplicated spelling, called at all four sites (the entry site's four
inline lines now route through it). Gate: `just check codegen-tool-reconfigure`
(`scripts/check-codegen-tool-reconfigure.py`, on the fast line, 15 self-test
cases on the normal path).

Proved by touch-and-reconfigure in the real shape — registration from inside a
function frame, called twice:

* REGISTER=OFF → tool absent from `RERUN_CMAKE`, artifact **not** re-emitted.
* REGISTER=ON  → tool present once (deduped), artifact re-emitted, and the next
  two `ninja` runs are `no work to do` (no reconfigure loop).

Mutation: reverting the helper call at each of the four sites individually
turns the gate red at that site, and the gate run against `origin/main` reports
all four.

### Widening cost — measured, and it is zero

Phase-424 forbids widening a watch set without pricing it against #0835. Of the
7 Zephyr build dirs in this checkout: 3 reach a configure-time emitter and all
3 **already** carry the CLI as a configure dependency via `nano_ros_entry()`;
the other 4 reach no configure-time emitter and gain nothing, because the
registration happens at the emitter's own call site. **No build dir here gains a
reconfigure.** What changes is that freshness stops being an accident of which
other function the image happened to call.

### Why the key is the binary and not `nros codegen-fingerprint`

Option (2) above is still the better key where it applies — the fingerprint
hashes what the emitters PRODUCE, and 41 distinct `nros` binaries map to 9
fingerprints, so 78 % of `setup-cli` rebuilds would cost nothing. Two reasons
it is not this change:

1. **Its corpus covers the message/service/action emitters only** — not
   `codegen entry`, not `codegen-system`. Keying those two on it would report
   FRESH for a real change to their emitters, which is the failure mode being
   closed. One key for four sites, and it has to be the conservative one.
2. **A configure cannot write its own configure input.** A fingerprint-keyed
   file needs a producer OUTSIDE the build dir, on the same event that rebuilds
   the CLI (`just setup-cli` / `scripts/bootstrap.sh`), or the first configure
   after a CLI rebuild never happens and the narrowing becomes a false FRESH.

That producer plus a fingerprint-keyed key for the interfaces site (only) is
the remaining work.

### Still open

* The reported symptom — three build stops from the stale-CLI refusal — is
  **unchanged**, and one of them is a genuine false stop: measured, appending a
  comment to `packages/cli/nros-cli-core/src/cmd/doctor.rs`, a file that cannot
  affect any emitted byte, makes every consumer `nros codegen` refuse. The
  refusal's watch set is the whole CLI closure.
* Narrowing that watch set is **rejected for now**, deliberately. The emitters
  reach through `cargo-nano-ros` and `nros-cli-core`, so a crate-level codegen
  closure is very nearly the whole CLI closure and would not have excluded
  `cmd/doctor.rs` anyway; and issue 0604 measured a hand-rolled closure walk
  wrong in both directions at once. A narrowing that is wrong is museum
  generated code reported as success — strictly worse than the stop.
* Option (3), one-step recovery, is untouched.
* `integrations/platformio/nros_codegen.py` runs `codegen-system` per build with
  no stamping and a soft failure. PlatformIO has no reconfigure model for the
  cmake helper to hook, so it is out of this fix's reach and stays a known gap.

## FOURTH SITE CLOSED 2026-09-05 (phase-424) — the `generated/` regeneration stamp had no edge at all

The survey above is right about the three cmake consumers. There is a FOURTH
place in the chain, on the shell side rather than in cmake, and it had no edge
at all.

### The place with NO edge: the Rust `generated/` tree

`scripts/build/codegen-stamp.sh` decides whether a leaf's cached `generated/`
survives, and its whole watch set was one file:

```
$ sed -n '/_codegen_stamp_sources/,/^}/p' scripts/build/codegen-stamp.sh
packages/core/nros-core/src/action.rs
```

That answers "has the SHAPE the generated code must fit changed?" and never
"has the code that GENERATES it changed?". Its own header even said so — *"Hard
constraint (CLAUDE.md): we do not touch nros-cli's codegen logic"* — a rule from
when the CLI was a submodule.

Measured against three real CLI builds in one worktree:

| CLI change | binary sha256 | `codegen-fingerprint` | stamp (old rule) |
| --- | --- | --- | --- |
| baseline | `255516c0…` | `080aec7d…` | `ec4700ee…` |
| `packs/c/message.h.jinja` edited | `285098cb…` | `c763d69d…` | `ec4700ee…` **unmoved** |
| `cmd/doctor.rs` string edited | `35591e2a…` | `080aec7d…` | `ec4700ee…` |

A real emitter edit moved what the tool emits and the stamp did not notice.

Nine of the ten call sites re-run `nros sync` unconditionally, so there the stamp
only governs the removal of files codegen stopped emitting. The tenth is
`just/zephyr-ci.just:170`, the one whose sync is CONDITIONAL:

```sh
if [ FORCE ] || ! nros_pkg_sync_stamp_fresh "$pkg" "$stamp" || [ ! -d "$dir/generated" ]; then
```

Force, a changed `package.xml`, or an absent `generated/`. Edit an emitter and
none of the three fire, the stamp does not drift, `generated/` is not wiped, and
every Zephyr Rust leaf compiles message crates the PREVIOUS CLI emitted. That is
this issue's title, in the lane where it bites.

### The fix

`nros_codegen_stamp_compute` now also hashes `nros codegen-fingerprint`, in the
same `tool:nros\0<fp>\0` encoding the two `.inputsig` lanes use.

**Keyed on what the tool EMITS, which is phase-424's constraint and not a
detail.** Measured on this host 2026-09-05: **168 distinct `nros` binaries
against 11 distinct codegen fingerprints**. A binary-keyed stamp — this issue's
own option (2), "stamp the generated output with the CLI's source hash" — would
wipe and re-sync every leaf on the 157 rebuilds that emit identical code, and
the CLI *source stamp* would be worse again: it moves for an edit to
`cmd/doctor.rs` and for a `play_launch` submodule pin bump.

The ladder that resolves the fingerprint had been written twice
(`workspace-fixture-signature.sh`, `compile-check-signature.sh`) and had already
drifted (`-s` vs `-r` on the cache, `binary:$hash` vs `$hash` on the fallback).
A third copy was the wrong move, so it is now one helper,
`scripts/build/codegen-fingerprint.sh`, with the fallback prefix as a parameter
— both existing callers keep their exact signature bytes (verified: 110
workspace + 40 compile-check records, 0 differing).

Gate: `just check codegen-stamp-inputs` (`tests/codegen-stamp-tests.sh`, on the
fast line). It asserts BOTH halves, because each alone has a trivial wrong
implementation, and both mutations were run:

* delete the fingerprint term → cases B and E fail (this issue, reintroduced);
* substitute `sha256sum` of the binary → case C fails (phase-424's constraint,
  violated).

Its negative control re-applies the pre-fix rule to the same trees and requires
it to stay blind.

`check-export-f-closure` covers the make-leaf hazard the new helper creates:
removing `nros_codegen_fingerprint` from `fixtures-build.sh`'s `export -f` list
makes that gate fail by name.

### 0835 budget

Unchanged, and it must be said precisely: **no `.inputsig` signature moved.**
Every row was recomputed with HEAD's scripts and with the refactored ones and
compared — 110 workspace records and 40 compile-check records, **0 differing**
(0835 counted 94 + 40; the manifest has grown since). The watch set gains no
path and hashes no build output. The cost this fix does add lands on
`generated/` wipes, and it is bounded by the fingerprint's movement rate
(11 in 168) rather than the binary's.

One-time cost: the stamp's input set changed, so every existing
`<leaf>/generated/.codegen-stamp` mismatches once and each leaf re-syncs on its
next build. `nros sync` materialises crates without compiling, and nine of the
ten call sites were re-syncing on every build already.

### Why THIS site keys on the fingerprint and the cmake sites key on the binary

The two answers look contradictory and are not — read them against
"Why the key is the binary and not `nros codegen-fingerprint`" above, whose two
objections are both about the CMAKE sites specifically:

1. *the corpus covers the message/service/action emitters only.* That is exactly
   what this stamp governs — `nros sync` materialising a leaf's `generated/`
   message crates. It governs no `codegen entry` and no `codegen-system` output,
   so the corpus is not short for it.
2. *a configure cannot write its own configure input.* This is not a configure
   input. `nros_codegen_stamp_compute` runs in the shell, before `nros sync`, on
   the same event that would rebuild the CLI — there is no ordering hazard to
   design around.

So the conservative binary key stays right for the four configure sites, and the
emit key is right here. One issue, two sites, two correct answers; recorded
together so the next reader does not "unify" them.

The item under "Still open" is unaffected: the stale-CLI refusal's watch set is
still the whole CLI closure, and narrowing it is still rejected for the reasons
given there.
