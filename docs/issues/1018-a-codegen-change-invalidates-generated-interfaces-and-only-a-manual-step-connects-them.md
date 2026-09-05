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

## RESIDUE MEASURED 2026-09-05 (phase-429 / RFC-0090) — two of the three stops are CORRECT

This issue reports three stops. Phase-429 checked each rather than assuming the
refusal was simply too broad, and the answer is not the one the issue expects.

**Stop 1 — editing `rosidl-codegen`.** Correct, and the reason the check exists.

**Stop 2 — moving a submodule pin forward.** ALSO CORRECT. The `play_launch` pin
is a genuine CLI build input: `build.rs` bakes it as `NROS_PLAY_LAUNCH_SHA` and
the issue-0409 guard compares that value. Issue 0561 records what happens when
the stamp is blind to it — a pin move left the stamp unchanged, `setup-cli`
skipped the rebuild while reporting success, and no sanctioned command could
clear the resulting mismatch. "Nothing in the consumer's tree changed at all" is
true and beside the point: something in the CLI's tree did.

**Stop 3 — editing `cmd/doctor.rs`.** The stamp asks *"does this binary match its
sources"*, and for that question the answer is right: `doctor.rs` is compiled
into the binary. It is the wrong question to ask before codegen — but the right
one, *"would this binary emit different bytes"*, cannot be answered without
compiling the sources, which is the thing the refusal exists to avoid.

### What was actually removable

One watch-set entry that provably could not affect an emitted byte:
`packages/cli/rosidl-codegen/templates/`, five `.jinja` files byte-identical to
`packs/scaffold/` and referenced from no `.rs`. `source_stamp.rs` scans `.jinja`,
so editing them stopped every consumer build; `codegen_fingerprint` hashes
`bundled_packs()` and correctly ignored them. Deleted, with both measurements as
proof:

    codegen-fingerprint  080aec7d…  ->  080aec7d…   (unchanged: nothing emitted moved)
    source-stamp         453a9ca4…  ->  ed29eacb…   (moved: five fewer watched files)

### What was NOT done, and why

* **Auto-rebuild** — this issue's option (1). Still rejected, by the refusal's own
  rule: compiling at build/test time is forbidden, and a consumer build that can
  compile a Rust binary is a surprise on the Zephyr lane.
* **Narrowing the closure** — issue 0604 measured a hand-rolled walk wrong in
  both directions at once. A narrowing that is too narrow is museum code reported
  as success, which is worse than a stop.

### What changed instead: who pays

RFC-0090 gives generated code a version the runtime asserts, so the refusal stops
being the only guard. It cannot fire for a user at all — `checkout_root_of`
matches only a binary inside `<root>/packages/cli/target/**`, and a user with a
released binary has no CLI sources. Verified: the same binary copied outside the
checkout runs the guarded verbs without refusing.

So this issue's residue is the price of a correct guard, paid by contributors
only. That is a materially different claim from the one the issue opens with, and
it is why the remaining stops are not being "fixed".
