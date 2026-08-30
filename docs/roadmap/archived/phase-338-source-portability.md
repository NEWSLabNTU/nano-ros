# Phase 338 — Source portability: one body per (language, program)

**Goal:** the user-authored source of an example is **identical across every
supported target**, with exceptions declared as data rather than tolerated as
silence. Then the per-platform copies can be folded, because nothing is lost by
folding them.

**Implements:** the portability half of RFC-0026 (examples are standalone
copy-out projects — this phase makes them *the same* project).
**Related:** [phase-329](phase-329-test-taxonomy-completion.md) (the gate lands
in its guards bucket), [phase-337](phase-337-board-support-matrix.md) (which
targets we promise; this phase is what makes one source reach them),
[RFC-0064](../design/0064-board-support-organization.md) (the same
one-source-many-targets principle, applied to board crates).

**Status.** DRAFT — not started.

---

## The finding that shapes this phase

**The portable shape already exists in-tree, and it is already ceremony-free.**
The `-entry` example variant is, in full:

```rust
// src/lib.rs
#![no_std]
pub use freertos_rs_talker::register;

// src/main.rs
nros::main!();
```

No `force_link_backend!`, no `<board>_app_main!`, no `extern crate <board> as _`.
The `nros::main!()` macro and the generated entry own all of it, and it is proven
on three platforms today (freertos, nuttx, threadx-linux). Across those three the
`main.rs` bodies are identical and the `lib.rs` bodies differ **only in the
re-exported node-package name** (`freertos_rs_talker` vs `nuttx_rs_talker`) —
a naming choice, not a technical requirement.

So this phase is **a migration to a shape that already works**, not an invention.
The generator already has what it needs: `emit_rust.rs` emits both the hosted
`fn main()` and the embedded `#[unsafe(no_mangle)] extern "C" fn main()`, and it
already resolves the board through `board_path_for` (`emit_rust.rs:27-29,189`).

## Measured baseline (2026-08-04)

**C and C++ are already portable, including across the native/embedded boundary:**

| program | shared body | platforms sharing it |
|---|---|---|
| `c/talker` | 89 loc | **6 of 7** — incl. native (verified byte-identical vs nuttx) |
| `c/action-server` | 167 loc | 5 of 6 incl. native |
| `c/service-server` | 107 loc | 5 of 6 incl. native |
| `c/service-client` | 103 loc | 5 of 6 incl. native |
| `cpp/action-server` | 76 loc | 5 of 6 incl. native |
| `cpp/talker` | 62 loc | 5 of 6 incl. native |
| `cpp/service-server` | 53 loc | 5 of 6 incl. native |
| `cpp/listener` | 46 loc | 5 of 6 incl. native |

**Rust is portable within groups, and the gaps are pure ceremony.** `rust/talker`
spans 9 platforms:

| body | loc | platforms |
|---|---|---|
| RTOS group | 34 | freertos, nuttx, esp32-baremetal, threadx-linux |
| bare-metal group | 38 | qemu-arm-baremetal, stm32f4 |
| threadx-riscv64 | 37 | = RTOS body **+3 lines** |
| zephyr | 40 | = RTOS body **+6 lines** |
| native | 91 | hosted `main.rs`, a different shape entirely |

The deltas, verbatim:

```rust
// threadx-riscv64, +3
extern crate alloc;
extern crate nros_board_threadx_qemu_riscv64 as _;
nros_board_threadx_qemu_riscv64::cyclonedds_app_main!(register);

// zephyr, +6
extern crate zephyr;
#[cfg(feature = "rmw-zenoh")] nros::force_link_backend!(nros_rmw_zenoh);
#[cfg(feature = "rmw-xrce")]  nros::force_link_backend!(nros_rmw_xrce_cffi);
nros::zephyr_component_main!(Talker);
```

Every line is the staticlib-DCE pitfall already in CLAUDE.md — no POSIX-style
ctor sections on RTOS targets, so backend registration must be an explicit call
and the entry macro must name the board crate. **None of it is user logic.**

**Not every divergence is a defect.** `c/listener` native is 113 loc against the
embedded group's 97 because native carries an `NROS_SUB_TYPE` env switch letting
tests select int32 vs string. That is a deliberate test affordance and must be
*declared*, not deleted.

---

## W1 — The portability gate (FIRST, before changing any source)

Land the measurement before the fixes, so every later wave has a scoreboard and
nothing silently regresses.

- [x] **W1.a** A guard test that, for each (language, program), normalizes every
      platform copy (strip comments and blank lines, collapse whitespace) and
      asserts the copies within a **portability group** are byte-identical.
      Source comparison only — no fixture, no boot, no QEMU. Belongs in
      phase-329's guards bucket.
- [x] **W1.b** **Exceptions are DATA with a reason, and they do not escape the
      gate — they form their own group.** Two kinds:
      - **Shape exceptions** — Zephyr's component convention (`Talker.c`,
        `Talker.hpp`, 34 loc against C's 89). Zephyr users expect that shape;
        this is a permanent, declared exception. The gate still asserts every
        Zephyr copy is identical *to the other Zephyr copies*.
      - **Affordance exceptions** — native's `NROS_SUB_TYPE` hook. Declared with
        the test that needs it, so deleting the test deletes the exception.
      An undeclared divergence is a failure. A declared one is a row with a
      reason — the same rule `Tier::CarveOut` already follows.
- [x] **W1.c** Record the baseline table above as the gate's starting state, so
      W2–W5 are measurable as groups merging rather than as vibes.

## W2 — Migrate the plain Rust examples onto the `-entry` shape

> **Measured 2026-08-04, and it changes this wave's shape. Read before starting.**
>
> The ceremony is **not** spread across the plain examples. Four of the six
> group-A platforms — freertos, nuttx, esp32-baremetal, threadx-linux — already
> have ceremony-free node packages whose `lib.rs` bodies are byte-identical. Only
> **two** platforms carry it, and each for a structural reason:
>
> * **threadx-riscv64** has no `-entry` package. Its `src/main.rs` is already the
>   clean `nros::main!()` form, but `src/lib.rs` additionally ends in
>   `cyclonedds_app_main!(register)` because the **CycloneDDS/CMake path builds
>   the lib as a staticlib** and needs an `app_main` symbol — `main.rs` is never
>   compiled on that path. Real requirement, wrong location.
> * **zephyr** likewise has no `-entry` package, so its `lib.rs` carries
>   `extern crate zephyr`, the two `force_link_backend!` arms and
>   `zephyr_component_main!`.
>
> Only three platforms ship `-entry` packages at all (freertos, nuttx,
> threadx-linux). So the naive fix — give every platform an `-entry` package —
> would *add* example directories, which cuts against W6.
>
> **The rule this suggests instead: node logic and entry glue live in separate
> files, and the gate compares the LOGIC file.** Every platform already
> separates them except where noted: baremetal is `lib.rs` (logic) + `main.rs`
> (glue), the `-entry` platforms are node pkg + entry pkg, threadx-riscv64 mixes
> glue into `lib.rs`, and native mixes everything into `main.rs`. Portability is
> a property of the logic; the glue is platform-specific by nature and the point
> is to **isolate** it, not to pretend it can vanish.
>
> **DECIDED (maintainer, 2026-08-04): option (b), AND collapse the 18 existing
> `-entry` packages.** The entry glue is a declared per-platform file inside the
> same package, and the separate `-entry` directories go away rather than
> spreading to the platforms that lack them.
>
> *(Recorded 2026-08-05. The decision was made in conversation and never written
> down, so this block still read "open question" for a day while work proceeded
> on the assumption it was open. That cost a wrong scoping note in W3.e — see
> the correction there.)*
>
> The target is the shape **native already has**: ONE package holding both
> `src/lib.rs` (node logic) and `src/main.rs` (`nros::main!()`), with
> `[package.metadata.nros.entry]`, `[[bin]]` and `[lib]` in the same
> `Cargo.toml`. threadx-riscv64 and zephyr already have no `-entry` package, so
> after the collapse the shape is uniform across all eight platforms instead of
> 3-of-8 carrying an extra directory.
>
> **Scope of the collapse, measured 2026-08-05:**
>
> | | count |
> |---|---|
> | `-entry` packages | **18** — 6 programs × {qemu-arm-freertos, qemu-arm-nuttx, threadx-linux} |
> | `examples/fixtures.toml` rows naming an `-entry` dir | 12 |
> | test files resolving a `*_rs_*_entry` binary | 6 |
>
> The six test files are the reason this is a wave and not a cleanup:
> `entry_e2e.rs`, `nuttx_entry_build.rs`, `threadx_linux_entry_build.rs` and
> `freertos_run_plan_runtime.rs` exist to exercise the entry-package shape
> specifically, plus the `binaries/{nuttx,threadx_linux}.rs` resolvers. Each
> needs to follow the binary to its new home or be retired with a reason.
>
> **Stage it one platform at a time** (maintainer's standing principle — "we
> don't migrate all boards at once, it reduces the blast radius"). threadx-linux
> first: its cells are `Runtime`, so the collapse can be proven by running the
> pairs rather than by building them.

The ceremony moves into the generated entry, where `nros::init`, executor open,
RMW registration and the spin loop already live.

- [~] **W2.a** SUPERSEDED by the option-(b) decision — the ceremony went into a
      declared per-platform glue FILE, not the generated entry. Kept for the
      record because the reasoning still applies if (a) is ever revisited.
      ~~Teach the generated entry to emit what the examples currently~~
      hand-write: `force_link_backend!` for the selected RMW(s), the board's
      `*_app_main!` / `zephyr_component_main!` invocation, and the
      `extern crate <board> as _` / `extern crate alloc` link anchors. The
      generator already knows the board (`board_path_for`), so this is emission,
      not new resolution.
- [~] **W2.b** SUPERSEDED — went the OTHER way. The decision was to collapse the
      `-entry` packages, not to spread them, so the plain examples gained a
      `src/main.rs` in the SAME package rather than an `-entry` sibling.
- [x] **W2.c** RESOLVED BY DELETION — the collapse removed every
      `pub use <node>::register;` file, so there is no line left to normalize
      and no naming policy to settle.
      ~~Normalize the node-package names so the `pub use <node>::register;`~~
      line is identical across platforms (today `freertos_rs_talker` vs
      `nuttx_rs_talker`). Naming rules already exist in
      `examples/workspaces/README-layout.md`; extend them to examples.
- [x] **W2.d** Normalize the `#![no_std]` / `#![no_main]` inconsistency — the
      freertos `talker-entry/src/main.rs` carries both attributes while the nuttx
      and threadx-linux ones carry neither, for the same generated entry.
      *Result:* threadx-riscv64 and zephyr join the RTOS group;
      `rust/talker` goes from 4 bodies to 2 (RTOS+baremetal, and native).

## W3.d — threadx-riscv64 converged (2026-08-05)

All six `qemu-riscv64-threadx/rust` bodies now normalize identical to the
group-A body. Divergence entries: **31 → 28**.

**Recorded because the first diagnosis was wrong.** An earlier pass read
`mod app_main;` in each `lib.rs`, concluded it was glue the gate still counted
as logic, and proposed teaching `nros::node!(Ty)` to emit the board glue so the
line could disappear. That macro change was never needed:
`example_portability::normalize` has skipped glue-module declarations since W1,

```rust
// A glue-module declaration is glue, not logic (see GLUE_MODULES).
if GLUE_MODULES.iter().any(|m| t == format!("mod {m};") || ...) { continue; }
```

so talker, listener and action-client already matched — all five group-A talker
copies normalize to the same 1087 bytes. Reading the diffs without re-reading
the normalizer turned "already solved" into a proposed macro rewrite. When a
gate says a thing is identical and the raw files disagree, the normalizer is the
thing to read first.

What actually differed on the other three was body drift, converged onto the
group body. The action-server case is the only one that lost behaviour: riscv64
computed a real iterative Fibonacci with 256-byte feedback buffers where the
group body publishes a fixed `[0,1,1]`. That was safe only because the cell is
`BuildOnly` — phase-182.5 dropped ThreadX riscv64 action from the run matrix on
wall-clock grounds, so the richer body had never executed.

**Open question this surfaces, deliberately not folded in:** the group body's
action-server is a stub. riscv64's was the better implementation, and
converging moved four platforms' worth of nothing while deleting the only real
one. Growing the GROUP body to compute the sequence is the honest fix, but it
touches four platforms with live runtime lanes and raises feedback buffers
128 → 256 on constrained targets. Worth its own item.

## W3.e — threadx-linux bodies converged (2026-08-05)

The four plain bodies (`action-client`, `action-server`, `service-client`,
`service-server`) now match the group-A body. Divergence entries: **22 → 18**;
fully-identical triples **10 → 14**.

**Verified by execution, not by building.** Unlike riscv64's action cell
(`BuildOnly`), `matrix::CELLS` runs ThreadxLinux × Rust × Zenoh ×
{Pubsub, Service, Action} as `Runtime`, so both pairs were driven live against a
real zenoh router on their declared locators. Service: ready marker → incoming
request → `a: 2 b: 3` → `Result of add_two_ints: 5`. Action: ready → goal request
→ executing → publish feedback → goal succeeded, with the client logging
`Goal accepted`, feedback `[0,1,1]` and result `[0,1,1]`.

The service pair is the one that *needed* running. Converging replaced an
unpaced per-tick retry that failed silently with the group's 1 s timer plus a
logged failure arm — that changes when the first call lands, and reading the
code cannot tell you whether it still beats the harness timeout.

**A coupling worth remembering for the remaining platforms:**
`[package.metadata.nros.node] class` names the node struct, so renaming the
struct without moving `class` leaves codegen pointing at a type that no longer
exists. `name` moved too, and doing so exposed that it had read
`"service_client"` while the body's own
`NodeOptions::new("add_two_ints_client")` said otherwise — the metadata had
disagreed with its own node all along.

The `-entry` siblings needed no change: they re-export by CRATE name
(`pub use threadx_linux_rs_service_client::register`), not by struct name.

### What is left on threadx-linux — resolved by DELETION, not by naming

Six `-entry` divergences remain, each a single line whose only difference is
that the node-crate name encodes the platform:

```rust
pub use freertos_rs_talker::register;       // qemu-arm-freertos
pub use threadx_linux_rs_talker::register;  // threadx-linux
```

**Correction (2026-08-05):** an earlier version of this section proposed
unifying example package naming (W2.c/W2.d) to make that line identical. That is
the wrong fix. The maintainer decided on 2026-08-04 to **collapse the 18
`-entry` packages** into their node packages, native-style — see the W2 decision
block. Under that decision these six files cease to exist, so there is no line
left to converge and no naming policy to settle.

The mistake was mine and it was avoidable: the decision existed, it just was not
written in the phase doc, and I scoped the remaining work from the doc rather
than asking. Recorded here because "the doc said it was still open" is exactly
how a settled decision gets relitigated.

## Status — what is left (2026-08-05)

Progress metric: divergence entries **41 (baseline) → 6**; 14 of 20
`(lang, program, group)` triples fully identical. **Every Rust group-A program
is byte-identical across all six of its platforms** — the portability claim,
demonstrated rather than asserted.

Landed: W1 (gate), W2 (collapse + naming resolved by deletion), W3.a/W3.b,
W3.d (riscv64), W3.e (threadx-linux), W4, W5.

### The 6 remaining divergences

| entry | class | closes with |
|---|---|---|
| `c/action-client`, `cpp/action-client`, `cpp/service-client` [qemu-arm-nuttx] | a 3-attempt retry loop the other platforms lack | a decision: retry belongs in the RMW, or in every copy. NOT a platform constraint. |
| `rust/talker`, `rust/listener` [qemu-esp32-baremetal] | group B is not internally consistent | W3.c, which is gated on W7 |
| `c/listener` [native] | **PERMANENT** — the `NROS_SUB_TYPE` env switch tests use to pick int32 vs string | nothing; delete only if that test goes |

So five are closable and one is permanent. The floor for this phase is **1**.

### The 4 remaining waves, honestly priced

* **W2.d** — the `#![no_std]`/`#![no_main]` split. freertos `main.rs` carries
  both, nuttx and threadx-linux carry neither. Small, and now cosmetic-only:
  `main.rs` is GLUE to the gate, so it is not compared and this blocks nothing.
* **W3.c** — measure group B's irreducible delta. Gated on W7.
* **W6** — **CLOSED 2026-08-05.** Not folded: portability is proven by a check,
  copy-out is preserved by keeping the copies. Both halves are now gated — the
  identical-copies test, and a new standalone-workspace-root test that codifies
  the copy-out property nothing had been checking.
* **W7** — one logging facade. **Needs re-scoping before it starts**: W7.a's
  premise was disproved (issue 0420 is archived not-a-bug; all three platforms
  do emit through the facade). What survives is W7.b/c/d — move the bodies to
  `nros_info!`, decide `log`'s status, re-measure group B — plus the real
  finding underneath 0420: the NuttX cells cannot run at all on an unprovisioned
  host, so a facade regression there would be invisible. Fix the visibility
  first or W7.b lands unwitnessed.

### Not phase-338, but surfaced by it

* The group action-server body is a stub publishing a fixed `[0,1,1]`;
  converging riscv64 deleted the only real Fibonacci implementation. Growing the
  group body is the honest fix — four platforms with live runtime lanes, and
  128 → 256 feedback buffers on constrained targets.
* Building any embedded example by hand needs SDK env `activate.sh` never sets
  (4 vars threadx-linux, 1 nuttx, 5 freertos), all defaulted only in
  `just/sdk-env.just`. Works through the `just` door, fails through the `cargo`
  door in a way that reads like a code fault.

## W2 DONE — the 18 `-entry` packages are collapsed (2026-08-05)

Every `examples/<plat>/rust/<prog>-entry` folded into its `<prog>` sibling.
All eight platforms now carry the same shape: ONE package with `src/lib.rs` and
`src/main.rs`. Divergence entries **18 → 6**; identical triples **14**; every
Rust group-A program is now byte-identical across the group.

`[[bin]]` took the short program name (`talker`), native's convention.

**The six `-entry` divergences did not need converging — they needed deleting.**
The re-export line that differed across platforms
(`pub use <plat>_rs_talker::register`) exists only because a separate entry
package needs to reach the node package's `register`. One package doesn't:
`nros::main!()` dispatches to the current crate's `register`, which
`nros::node!` emits in that same crate. The gate then flagged all 12 stale
entries by itself — "no such example — stale entry, delete it" — which is the
ratchet doing exactly what W1 built it for.

### Verification, and what it does NOT cover

| platform | evidence |
|---|---|
| threadx-linux | all 6 build; service + action pairs driven LIVE against a zenoh router |
| qemu-arm-nuttx | all 6 `cargo check`; **no link, no run** |
| qemu-arm-freertos | all 6 `cargo check`; **no link, no run** |

Two thirds is compile-verified only. A full nuttx link needs the board-centric
image build (a configured NuttX); freertos needs the kernel compile through the
`just` overlay. Neither is available on the host this landed from, so the
freertos/nuttx lanes want a real run before this is called proven.

### Two things this surfaced

**1. The nuttx leaf configs were missing their board patch.** `nros sync` had to
add `nros-board-nuttx-qemu = { path = … }` to all six `.cargo/config.toml`
files. The `-entry` packages had depended on that crate while their patch table
never named it, so the dep resolved only inside a `just` recipe that supplies
more context. Collapsing surfaced it because a bare `cargo check` in the leaf
finally had to resolve it alone.

**2. Building any embedded example by hand needs SDK env no activate.sh sets.**
threadx-linux needs `THREADX_DIR`, `NETX_DIR`, `THREADX_CONFIG_DIR`,
`NETX_CONFIG_DIR`; nuttx needs `NUTTX_DIR`; freertos needs `FREERTOS_DIR`,
`FREERTOS_PORT`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR`, `NROS_LAN9118_LWIP_DIR`.
All are defaulted in `just/sdk-env.just`, so the `just` recipes work and a
direct `cargo` invocation does not. This is the same shape as the NuttX-cells-
always-skip finding recorded against issue 0420: the build works, but only
through one door, and the other door fails in a way that reads like a code
fault. Worth its own issue rather than folklore.

## W3 — Close the native/embedded Rust split

> **Measured 2026-08-04, and the premise was wrong. Read before starting.**
>
> This wave assumed native's 91-line `main.rs` was an *unsplit* version of the
> group's 36-line body — logic plus ceremony the generated entry already owns.
> It is not. Native's rust examples are a **different example class**:
>
> * **Every** native rust example declares `[package.metadata.nros.application]`
>   and uses the imperative Executor API (`Executor::open`, `create_node`,
>   `register_timer`). **Zero** are Node-class.
> * **Every** embedded copy declares `[package.metadata.nros.node]` and uses the
>   declarative API (`impl ExecutableNode`, `nros::node!`).
> * `example_shape.rs` asserts this "Node XOR Application classification"
>   deliberately — it is a designed distinction, not drift.
>
> So closing the split as written would **rewrite the reference platform's
> examples from one public API to another**, which is a product decision, not a
> portability fix. The 6 native rust divergences are reclassified in the gate as
> a class difference with that reasoning recorded.
>
> **Note what this does NOT excuse.** C has no such divide: `c/talker` is one
> 89-line body across native and five embedded platforms. So the two-API split
> is specific to Rust, and it is fair to ask whether it should exist — a Rust
> user reading the native talker and then the FreeRTOS talker sees two different
> ways to write the same node.
>
> **Open decision, for the maintainer:**
> (a) give native a Node-class sibling example so the group has a peer and both
>     APIs stay documented; (b) migrate the native examples to Node-class and
>     keep the imperative API for genuinely application-shaped demos only;
> (c) accept two Rust authoring APIs as intended and keep the exception
>     permanent. Nothing here is blocked on the answer except W3 itself.

C already proves this gap is closable: `c/talker` shares one 89-line body across
native *and* five embedded platforms. Rust splits a 91-line hosted `main.rs` from
a 34-line `lib.rs`.

### W3 execution plan — DECIDED (option b, maintainer 2026-08-04) and fully mapped

**Decision:** migrate the native standalone examples to Node-class so `talker`
means one thing everywhere, and keep the imperative API documented by one or two
genuinely application-shaped examples (`custom-transport-*`, a host-side tool).

**This finishes work phase-277 W4 started.** `bins/header-chatter-talker`'s own
doc says it was "moved out of `examples/native/rust/talker` … so the example
stays cfg-free", and `bins/int32-sink`'s says it was "moved out of
`examples/native/rust/listener` in phase-277 W4". The destinations already exist;
the `NROS_PUB_TYPE` / `NROS_SUB_TYPE` switch simply survived in the examples.

**Step 1 — repoint the 7 affordance sites.** Measured; do this FIRST, so the
examples are free to change.

| test | line | env | drives | repoint to |
|---|---|---|---|---|
| `declarative_bridge_zenoh_to_cyclonedds` | 72 | `SUB_TYPE` | **C** listener | keep — C affordance is a declared permanent exception |
| `declarative_bridge_zenoh_to_cyclonedds` | 82 | `PUB_TYPE` | rust talker | an Int32 talker bin |
| `declarative_bridge_zenoh_to_xrce` | 103 | `SUB_TYPE` | **rust** listener, XRCE build | **BLOCKED** — see below |
| `declarative_bridge_zenoh_to_xrce` | 116 | `PUB_TYPE` | rust talker | an Int32 talker bin |
| `esp32_emulator` | 514 | `SUB_TYPE` | rust listener | `bins/int32-sink` |
| `zephyr` | 1611 | `SUB_TYPE` | rust listener | `bins/int32-sink` |
| `ros_editions_e2e` | 168 / 191 | both | rust talker + listener | Int32 talker bin + `int32-sink` |

**The marker changes with the binary, and that is the trap.** The rust listener
prints `LISTENER_LOG_PREFIX` (`"I heard:"`) in every mode; `int32-sink` prints
`INT32_LISTENER_LOG_PREFIX` (`"Received:"`). Every repointed assertion must move
to the matching constant — `esp32_emulator` alone has ~6 sites. Never a literal
(CLAUDE.md).

**Step 1 status (2026-08-04): 2 of 8 sites done; one is BLOCKED.**

- **DONE** — both bridge `PUB_TYPE` sites now spawn `bins/header-chatter-talker`
  (it publishes Int32 on `/chatter` natively) instead of the example with
  `NROS_PUB_TYPE=int32`, and their readiness wait moved
  `TALKER_LOG_PREFIX` → `INT32_TALKER_LOG_PREFIX` because that bin prints
  `"Published:"`, not `"Publishing:"`. Compiles clean.
- **Re-counted: 8 sites, not 7.** `declarative_bridge_zenoh_to_xrce:103` drives
  `xrce_listener_binary`, which resolves to `build_example_rmw("native/rust/
  listener", …, Rmw::Xrce)` — the **rust** listener, not the C one. It was
  mis-attributed by analogy with the cyclonedds file.
- **BLOCKER for that site:** `bins/int32-sink` is hardcoded to zenoh (a direct
  `nros-rmw-zenoh` dep, no rmw feature axis), so it cannot stand in for an XRCE
  listener. **Step 1 cannot complete, and therefore step 2 cannot strip the
  listener's branching, until `int32-sink` gains the same `rmw-{zenoh,xrce,
  cyclonedds}` feature axis the examples already have** — plus a fixture row and
  resolver per RMW. Mechanical, but it is prerequisite work nobody had costed.

**Step 2 — strip the branching** from `examples/native/rust/{talker,listener}`.
Two-thirds of the talker's 91 lines is the same demo written three times
(Header / Int32 / String); what remains is ~30 lines against the group's 36.

**Step 3 status (2026-08-04): ATTEMPTED on the talker, REVERTED, blocked on two
findings that only a real run surfaced.** The migration itself is easy — the
manifest swap (`[lib]` + `.entry` + `.node`, mirroring
`qemu-arm-baremetal/rust/talker`) and the body copy both worked first try, and it
compiled. Then:

1. **The generated hosted `main` does not spin by default.** `nros::main!()`
   emits `__nros_hosted_spin_if_requested`, an env-gated BOUNDED spin, so the
   process printed `application complete` and exited having published nothing.
   Fixed by `nros::main!(spin = "forever")` (issue 0274), which is what the
   imperative version's `spin_blocking(SpinOptions::default())` did. Not a
   blocker — just undocumented for standalone examples.

2. **BLOCKER — nothing installs a hosted logger.** With the spin fixed the
   process ran but emitted **no output at all**. The group's Node body logs via
   `log::info!`, and on native neither `nros-board-linux` nor `nros-board-linux`
   installs a `log` sink; the imperative example called `env_logger::init()`
   itself, and a Node body has nowhere to put that because `nros::main!()` owns
   `main`. Every test asserting `TALKER_LOG_PREFIX` would go silent.

   Note the facades already disagree, which the gate could not see because
   native was not comparable: the four group-A standalone copies use
   `log::info!`, while `workspaces/rust/src/talker_pkg` uses
   `nros_log::nros_info!(&DEFAULT_LOGGER, …)` — and *that* one works on native
   with no init, which is why the workspace fixture is green today.

   **Three ways out, and this is a product decision:**
   (a) `nros-board-linux`'s `BoardEntry::run` installs the hosted logger, which
   mirrors embedded exactly (the ThreadX family driver already calls
   `install_uart_logger::<B>()`); (b) the macro emits `env_logger::init()` for
   hosted deploys; (c) every Node body moves to `nros_log`, unifying the facade
   across standalone and workspace — arguably the real fix, since `log` needs a
   hosted init and `nros_log` does not, but it touches the four group-A copies
   and their asserted markers.

   (a) is the smallest and most consistent. Whichever wins, it changes the board
   or macro layer for *every* native entry, so it is not a change to slip into an
   example migration.

**Step 3 status update (2026-08-04): talker DONE; the other five are BLOCKED on
an output-contract mismatch, and it is not mechanical.**

`talker` migrated cleanly because its output contract is one line
(`"Publishing: '...'"`) that the group body already emits. The other five do
not have that property. Measured across all seven native-only markers sampled,
**every one is asserted somewhere in the test suite**:

| native-only marker | asserted in |
|---|---|
| `Timed out waiting for /add_two_ints service` | `services.rs:96,272` |
| `Waiting for service requests` | `output.rs::SERVICE_SERVER_READY_MARKER` + zephyr |
| `Waiting for action goals` | 2 files |
| `Subscriber created` | 3 files |
| `Spin error` | 2 files |
| `Action server not confirmed within 10s` | 1 file |
| `Failed to create action client` | 1 file |

Two are load-bearing, not incidental:

* **`services.rs:96` asserts a FAILURE MODE.** Its comment: "Without a server
  the client must report a failure (and exit non-zero) rather than hanging or
  crashing." The group's Node body does the opposite — `Err(_) => {}` silently
  retries forever — so the test would hang, then fail.
* **`SERVICE_SERVER_READY_MARKER` is a named constant** in `output.rs`, used as
  a cross-platform readiness gate (zephyr keys on it for C/C++). The rust group
  bodies never emit it, so a migrated native server would stop being detectable
  as ready.

So the native examples carry a **richer output contract than the group bodies**,
and the suite depends on it. Copying the group body over them is not a port —
it is a silent capability loss.

**Three ways forward, for the maintainer:**

(a) **Enrich the group bodies to native's contract** and then migrate. Every
    platform gains the readiness/failure lines, which is arguably right — the
    embedded copies are the ones under-reporting, and `SERVICE_SERVER_READY_MARKER`
    already exists as a cross-platform concept the rust bodies fail to honour.
    Cost: four platform bodies change, plus a re-run of each platform's lane.

(b) **Move the failure-mode assertions to test bins**, the way the
    `NROS_PUB_TYPE` switch went in step 1. "Timed out waiting" is a failure-mode
    test, not example behaviour, and a dedicated bin could own it. Does not help
    the readiness marker, which genuinely belongs in the example.

(c) **Keep the five as declared divergences.** Honest, cheap, and leaves `talker`
    as the one program that means the same thing everywhere.

(a) is the real fix and the only one that ends with the five migrated. It is a
bigger change than this wave assumed, and it should not be started without the
per-platform lane time budgeted.

**Per-program findings (2026-08-04), re-measured properly.** The earlier
"every native-only marker is asserted" table was inflated by regex artifacts.
Checked one program at a time:

| program | verdict |
|---|---|
| `talker` | **DONE** — group body already had its whole contract |
| `service-server` | **DONE** — group body already emitted all three asserted markers |
| `action-server` | contract fine, but **BLOCKED at runtime** — see below |
| `listener` | needs ONE line added to the group body |
| `service-client`, `action-client` | not yet checked; both have real failure-mode assertions |

**`listener` — option (a), one line.** `native_api.rs:926` uses
`"Subscriber created"` as the rust listener's readiness gate
(`.expect("rust-listener did not become ready")`), and the group body emits
only `"I heard: [{}]"`. Adding the readiness line to the group body is additive
and harmless for embedded, whose readiness comes from the board banner.

**`action-server` — the output contract is NOT the problem; the hosted spin is.**
The group body already emits all five asserted markers, including
`ACTION_SERVER_READY_MARKER`, and the only gap (`"Error accepting goal"`) is
asserted nowhere. But the migrated server **declares every entity and then never
accepts a goal**: the client retries three times and reports "Goal was never
accepted", with no `"Received goal request"` on the server. Reverting to the
imperative version makes it work immediately, on the same router, first try.

Not a discovery-timing artifact — reproduced with 10 s of server head start.

**This is a runtime defect, not a migration problem, and it needs its own issue.
The CAUSE is not yet identified — my first diagnosis was wrong and is corrected
here rather than left standing.**

What is established:

* The imperative action-server works; the Node-class migration of the same
  program does not. Same router, same client, first try either way.
* Node-class actions DO work on native — the matrix carries four `Native` ×
  `Action` × `Workspace` Runtime cells, run through `nros::main!(launch = …)`.
* `talker` and `service-server` migrate cleanly; neither uses `tick()`.
  `action-server` does. `service-client` also does, so it likely fails the same
  way.

**Retracted:** I first blamed `__nros_hosted_spin_forever` for looping on
`spin_once` without `run_ticks`. That is wrong. `RuntimeCtx::runtime` is a
`&mut dyn NodeDispatchRuntime`, whose `ExecutorNodeRuntime` impl
(`node_runtime.rs:552`) delegates to the inherent `ExecutorNodeRuntime::spin_once`
(`:392`), and that DOES call `run_ticks()`. Ticks are driven on the hosted path.

**Narrowed, unverified:** `run_ticks` iterates `self.components`
(`node_runtime.rs:461-471`), which is populated only by the registration path at
`:350-351`. A node whose entities reach the executor but whose CELL never lands
in `components` would show exactly this signature — direct service/subscription
callbacks fire (service-server works) while component ticks never run (action
goal handling dead). Worth checking whether the standalone `nros::main!` register
path populates `components` the way the launch path does. Not confirmed; do not
treat as the answer.


**The action envelope mismatch — diagnosed 2026-08-04, NOT fixed, and it needs
its own issue + probably an RFC.** This is the second half of what blocked
`action-server`, after the type-name bug (fixed, `d63832006`).

With type names corrected, a goal is accepted, executed and succeeds — but the
typed client still fails feedback (`Transport(DeserializationError)`) and result
(`ServiceRequestFailed`). The cause is a **payload envelope difference, and it is
deliberate**:

`nros/src/node.rs` serializes raw action feedback and results **with a CDR
encapsulation header inside the envelope**, so the wire carries
`[outer header][goal_id][INNER header][body]`. Both sites document it, and the
result one documents the failure mode of removing it naively:

> "Without the header the reader eats the first data word (e.g. a sequence
> length) → empty/garbage payload (issue #35 M-F.23 follow-up: action result
> `sequence` deserialized to len 0)."

ROS 2 expects a **single** header. So the raw path is self-consistent — nano-ros
raw publisher ↔ nano-ros raw consumer agree — and wire-incompatible with both
ROS 2 and nano-ros's own TYPED path. Exactly the shape of the type-name bug, one
layer down.

**Why this is not a quick fix.** The raw CONSUMER is symmetric with the producer
— `action_core.rs::try_recv_feedback_raw` reads the outer header, then the
`goal_id`, and the body is read with `new_with_header` again — so removing the
producer's header alone reproduces exactly the corruption issue #35 documents.
Producer and consumer change together or not at all, and every action Runtime
cell is raw↔raw, i.e. precisely the pairs the change breaks and must re-prove on
real targets.

**Correcting an overclaim in the first draft of this note:** I recorded that the
generated C++ message exports encode this convention. They do not — their
`new_with_header` is ordinary per-message CDR, correct ROS 2 behaviour for a
topic payload. The blast radius is the action envelope specifically, not codegen.

Filed as [issue 0418](../issues/0418-action-payload-envelope-not-ros-compatible.md)
with [RFC-0069](../design/0069-action-payload-envelope.md) for the decision —
which envelope is canonical, since the interoperable answer breaks
nano-ros↔nano-ros across the version boundary.

**Consequence worth stating plainly:** raw-registered action servers and clients
have never been wire-compatible with ROS 2 on feedback or result payloads. The
type-name fix made them *discoverable*; this makes them *usable*. Until both
land, `action-server` / `action-client` / `service-client` cannot migrate to
Node-class on native, because the native counterparts use the typed path.

**Step 3 — migrate the six** (`talker`, `listener`, `service-{client,server}`,
`action-{client,server}`): `src/main.rs` becomes `src/lib.rs` carrying the Node
trait impls plus a one-line `src/main.rs` (`nros::main!();`), and `Cargo.toml`
swaps `[package.metadata.nros.application]` for `[package.metadata.nros.node]`
(with `class` / `name`) and gains a `[lib]` section. The target body already
exists to copy: `examples/workspaces/rust/src/talker_pkg/src/lib.rs` is the same
shape as the embedded standalone copies.

**Step 4 — verify.** Native is the reference platform (72 of 174 Runtime cells),
so this needs the native fixture family rebuilt plus the three repointed interop
families (esp32, zephyr, ros-editions) and both bridge lanes. Do not land steps
2–3 without it.

**Not in scope:** the C listener's `NROS_SUB_TYPE`. C has no Application/Node
divide (`c/talker` is one body across native and five embedded platforms), and
that affordance stays a declared exception.

**Step 4 found a latent CLI bug before it found anything about the migration**
(fixed in `afe0828f4`). The native lane died at ~55 minutes on:

```text
Error: read …/service-server/target-xrce/nros-fast-release/incremental/
       service_server-…/s-…-working/dep-graph.part.bin
Caused by: No such file or directory (os error 2)
  at nros-cli-core/src/orchestration/metadata_refresh.rs:392
```

The source-digest dirwalk skipped build output by matching the EXACT names
`target` and `build`, but this repo's convention is a per-RMW suffix —
`target-xrce`, `target-zenoh`, `build-zenoh`. So the digest recursed into
cargo's incremental artifacts and raced a scratch file cargo deleted from under
it. Nothing under `target-*` was ever a legitimate hash input.

Worth recording because of *why it waited for this phase*: a package's sources
are only hashed in the **Node** shape, so making the native examples Node
packages (steps 2–3) was what first pointed the walker at those trees. The
migration did not cause the bug; it was the first thing to execute it. Fixed as
a class — one `build_output::is_build_output_dir` predicate, prefix-matching,
called from both exact-match walkers (`metadata_refresh`, `check_workspace`).
`stale_guard` and `source_stamp` look similar but are not this class and were
left alone.

- [x] **W3.a** Establish why the hosted Rust example is 91 lines when its embedded
      sibling is 34 — how much is genuinely hosted-only (arg parsing, signal
      handling, `std` logging) versus ceremony the generated entry could own on
      both sides. **ANSWERED above: ~2/3 is the triple-implemented type switch.**
- [x] **W3.b** Bring native onto the entry shape so the body is the same 34 lines,
      with the hosted/embedded difference living entirely in the generated `main`
      that `emit_rust.rs` **already** emits in both shapes.
      *Result:* `rust/talker` reaches ONE body across **group A** (below) — the
      portability claim, demonstrated rather than asserted.

**Correction, measured 2026-08-04 — there are THREE groups, not one plus Zephyr.**
An earlier draft of this phase claimed all 9 platforms converge. Diffing
bare-metal against the RTOS body shows that is wrong, and the difference is not
ceremony:

```rust
// qemu-arm-baremetal, vs the RTOS body
const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;
fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
static LOGGER: Logger = Logger::new("talker");   // nros_log, not `log`
```

Bare-metal has **no RTOS scheduler**, so it uses deferred dispatch plus an
explicit tick loop, and it cannot use the `log` facade. That is a real execution
model, not a portability defect. So the target is:

| group | platforms | why it is its own group |
|---|---|---|
| **A — scheduled** | native, threadx-linux, freertos, nuttx (arm+riscv), threadx-riscv64 | an RTOS/OS scheduler runs callbacks |
| **B — bare-metal** | qemu-arm-baremetal, qemu-esp32-baremetal | deferred dispatch + explicit `tick()`, `nros_log` facade |
| **C — Zephyr** | zephyr | component authoring shape (`Talker.c`/`.hpp`) |

Part of B's delta *is* reducible — the `log` vs `nros_log` split is a facade
problem, and `DISPATCH`/`tick` could carry defaults. W3.c measures how much;
whatever survives is a declared group, not a defect.

- [x] **W3.c** Measure B's irreducible delta after unifying the logging facade and
      defaulting `DISPATCH`/`tick`. Record the remainder as group B's declared
      reason. **Do not force B into A** — an execution-model difference expressed
      as one body with cfg branches is worse than two honest bodies.

## W4 — Arch portability: the FreeRTOS Cortex-M4F/M7 panic

**Owner moved here from phase-337 W1.a** — it is a source-portability defect, so
it belongs with the rest of them. phase-337 keeps a pointer.

- [x] **W4.a** `packages/boards/nros-board-freertos/build.rs:273-287` **hard-panics**
      for any `thumb*` target that is not `thumbv7m` unless `FREERTOS_CFLAGS` is
      set, even though `config/freertos-lwip/nros-platform.toml` now lists
      `arch = ["cortex-m3", "cortex-m7"]`. Derive the cflags from the `[arch.*]`
      profile the platform config already carries; keep `FREERTOS_CFLAGS` as the
      rung-1 override. Every industrial FreeRTOS board is M4F/M7 (S32K344 is M7),
      so this is the difference between the matrix being reachable and being a
      claim.
      *Verify:* a compile-only `thumbv7em-none-eabihf` check in the embedded lane.

## W5 — Board-shaped branches that should be platform- or capability-shaped

Two places where a **board name** reaches a decision that is not about that board.
Both were found while auditing the net seam (RFC-0064 R3), which is otherwise
clean — one `net.c` per platform, per-board deltas expressed as weak symbols and
macro arguments.

- [x] **W5.a — DONE 2026-08-04.** `cmake/NanoRosFeatureSet.cmake` matched
      `_FS_BOARD STREQUAL "threadx-linux"` vs `"riscv64-qemu"` to pick the std vs
      `alloc`+`panic-halt` libc tier, and `FATAL_ERROR`d otherwise — so a third
      ThreadX board could not exist without editing this file.

      The tier turned out to need no new capability: it is exactly `_cross`,
      which the function **already computes**. threadx-linux is a host build;
      threadx-qemu-riscv64 is a cross build (its `[board.cmake] toolchain_file`
      sets `CMAKE_SYSTEM_NAME`). Deriving from `_cross` generalizes to any future
      ThreadX board with no edit here, and `BOARD` stops being load-bearing.
      Verified with a standalone `cmake -P` harness: both live boards produce
      **byte-identical** feature lists to the old branch, and a third board now
      resolves instead of fatalling.

- [x] **W5.b — MEASURED 2026-08-04; no untangling needed, and the earlier
      "blocks phase-337 W7.b" claim was too strong.** `orin-spe` does reach
      link-feature selection by name (`nros-zpico-build/src/runner.rs:112,240,
      344-345,434-435,543-544` + `LinkPolicy::orin_spe()` +
      `config/orin-spe/nros-platform.toml` + the `zpico-sys` `orin-spe` feature),
      which is the shape RFC-0064 flagged. But the chain is **self-contained**:
      the only crate that enables the `orin-spe` feature is
      `nros-board-orin-spe` itself (`its Cargo.toml:81`), and no example, no
      fixture and no test references it. The board crate's own comment already
      says "orin-spe is a FreeRTOS board, not a platform" (phase 121.10).

      So deleting the board crate makes the whole chain dead code rather than
      breaking anything. phase-337 W7.b is an **ordering**, not a dependency:
      delete the crate, then in the same change delete the now-dead
      `config/orin-spe/`, `LinkPolicy::orin_spe()`, the `CARGO_FEATURE_ORIN_SPE`
      branches, the `zpico-sys` feature, the `nros-sdk-index.toml [board.orin-spe]`
      entry and the `zpico_backend` lint value. Recorded here so W7.b does not
      re-derive it.

## Every embedded lane verified (2026-08-06)

| lane | cells | result |
|---|---|---|
| qemu-arm-nuttx | 9 — {pubsub, service, action} × {Rust, C, C++} | **pass** |
| qemu-arm-freertos | 9 — same | **pass** |
| qemu-riscv64-threadx | 6 — {pubsub, service} × 3 langs (action is `BuildOnly`) | **pass** |
| threadx-linux | pairs driven live earlier | pass |
| native | all three W8 programs run | pass |

The `-entry` collapse and the W8 wait-then-send change are now proven at
runtime on every platform that has runtime cells.

### freertos: the collapse had broken all three Rust cells

Three omissions, all from ONE root cause — **the merge script decided what to
carry from the entry manifest with incomplete rules**:

* **`ip` / `gateway` dropped.** The script carried only `rmw`, `domain_id`,
  `locator` from the entry's deploy block. FreeRTOS entries also set
  `ip = "10.0.2.15"` / `gateway = "10.0.2.2"`, because `LWIP_DHCP` is 0 on that
  board — the address is baked, not leased. Without them the image took the
  board default `192.0.3.10` and could not route to the router, so it booted,
  printed "Network ready.", and silently stopped short of "Application setup
  complete".
* **`nros-platform` dropped.** The fallback guard was
  `if "nros-platform" not in ntext` — a whole-file substring test. The freertos
  node manifest mentions `nros-platform/platform-freertos` in two COMMENTS, so
  the guard read as "already present".
* **`locator` lost to the node's value.** The rule was "add keys the node
  lacks", but when both blocks define a key the ENTRY's is the deployed one.
  The per-role ports (7800/7810/7820) lost to the node's generic 7447.

### The pattern worth carrying forward

**Substring tests against a whole file, three times now.** `"[[bin]]" in text`
matched a comment saying *"no [[bin]]"*; `"nros-platform" in text` matched a
comment; and a repair script matched `[package.metadata.nros.deploy.*]` inside
a `[features]` comment and appended keys to the wrong table. Structured edits
need anchored patterns (`^key =`, a real table header), never `in text`.

**And `git mv A/x B/x` nests when `B/x` exists** — that buried the NuttX cargo
config. A move script must assert the destination is absent.

Both classes are silent: they produce a plausible file that builds, and fail
only at runtime on hardware. Every one of them was found by RUNNING, and none
by reading.

### Two recurring environment traps, neither ours

* **Stale cmake caches.** nuttx, freertos and riscv64 all failed first on stale
  build dirs — nuttx on a `nros-board-nuttx-qemu-arm` path upstream removed,
  the other two on the sizes-header mirror (`EXECUTOR_OPAQUE_U64S` disagreeing
  between the C and C++ halves). Wiping the build dirs fixed all three. This is
  the issue-0268 class: incremental trees red, clean trees green.
* **A missing lock entry.** `nros-cpp` gained an `nros-bridge` dep that
  `nros-nuttx-ffi/Cargo.lock` never recorded, blocking the nuttx C++ lane.

## Embedded lane verification — nuttx PROVEN, and it found three breaks (2026-08-06)

Running `just nuttx build-examples` + the `rtos_e2e` NuttX cells was worth it:
**all nine nuttx cells now pass under QEMU** — {pubsub, service, action} ×
{Rust, C, C++} — plus `nuttx_entry_build::nuttx_entry_demos_build`. That is the
`-entry` collapse AND the W8 wait-then-send change proven at RUNTIME on the
platform whose retry loop started all of this.

Getting there surfaced three separate breaks. Two were mine.

### 1. `git mv` nested the entry's cargo config (MINE, fixed)

The collapse ran `git mv <entry>/.cargo <node>/.cargo`. The six NuttX node
packages already HAD a `.cargo/`, and git moves a directory INTO an existing
one — producing `<node>/.cargo/.cargo/config.toml`, which cargo never reads.
The node's own config survived, and it lacks the NuttX image-link recipe
(`-Tdramboot.ld`, `--entry=__start`, the kernel-lib `--start-group`) that
`nros-board-nuttx-qemu`'s build.rs documents as living in the ENTRY's config.

Every nuttx Rust example linked without NuttX's libc:
`undefined reference to open / socket / ioctl / malloc / __errno`.

**The lesson is the mechanical one:** `git mv A/x B/x` silently NESTS when
`B/x` exists. It hit `.cargo` and would have hit `src/main.rs` or `launch/`
just as quietly — those were only safe because no node dir had them. A move
script must assert the destination is absent, not assume it.

### 2. Six fixture rows that only made sense pre-collapse (MINE, fixed)

The bare nuttx rust rows (no `rmw`) built the LIB-ONLY package: no `[[bin]]`,
so cargo never linked and the row was harmless. The collapse gave that package
the entry's `[[bin]]`, and a build selecting no RMW links no board glue — so
the row started failing the same link. They were also redundant with the
`rmw = "zenoh"` rows carrying the baked locator env. Deleted.

Their presence also caused a second symptom worth naming: two rows on one dir
meant two concurrent `nros sync` passes over that dir, and the collapsed dirs
now have a `launch/`, so both raced staging the SystemModel —
`YAML: missing field meta` on `system_model.yaml.resolving`.

### 3. Two pre-existing blockers (NOT mine)

* **Stale cmake caches.** The nuttx C/C++ and workspace build dirs cached
  `NUTTX_FFI_CRATE_DIR` / `NUTTX_DEFCONFIG` under a `nros-board-nuttx-qemu-arm`
  path that upstream `983306561` (phase-337 W3, one board crate two witnesses)
  removed. The board cmake's own defaults are correct; only the caches were
  wrong. Wiping the 12 build dirs fixed it — the class upstream `84eb6d26b`
  (issue 0400) addresses.
* **A missing lock entry.** `nros-cpp` gained an `nros-bridge` dependency that
  `nros-nuttx-ffi/Cargo.lock` never recorded, so `--locked` refused the C++
  lane. Only the C++ graph pulls it in, which is why it sat unnoticed. Fixed
  via `just lock-update`.

### Still unverified

freertos and threadx-riscv64 embedded lanes. threadx-linux was proven earlier
by building AND running its collapsed examples; native by running all three W8
programs. Given that nuttx — the most intricate of the embedded lanes — is now
green end to end, the remaining two are lower risk, but they are not evidence.

## W8 — DONE 2026-08-06: C++ `wait_for_*` bound, client examples unified

The last three closable divergences (`c/action-client`, `cpp/action-client`,
`cpp/service-client`, all on qemu-arm-nuttx) are one pattern: a **3-attempt
retry loop** around the first request, spinning between attempts.

```c
for (int attempt = 0; attempt < 3; attempt++) {
    if (attempt > 0) {
        fprintf(stderr, "send_goal timed out; retrying (attempt %d)\n", attempt + 1);
        nros_executor_spin_some(&app.executor, 1000000000ull);
    }
    ret = nros_action_send_goal(...);
    if (ret != NROS_RET_TIMEOUT) break;
}
```

It is a workaround for slow discovery: on NuttX under QEMU the first request
fires before the server is visible, times out, and the retry — after a spin that
lets discovery finish — succeeds.

### The earlier note was wrong about the fix

It said *"unify by giving every copy the retry."* Don't. **The API already has
the right primitive**, and no example uses it:

| | C | C++ |
|---|---|---|
| `wait_for_service` | `nros_client_wait_for_service` ✅ | **missing** — `Client` has only the non-blocking `server_available()` |
| `wait_for_action_server` | `nros_action_client_wait_for_action_server` ✅ | **missing** — `ActionClient` has no readiness method at all |

Both C entry points document themselves as mirroring
`rclcpp::Client::wait_for_service` / `rclcpp_action::Client::wait_for_action_server`.
A grep of `examples/` for either name returns exactly one hit, and it is a Rust
one (`native/rust/service-client-callback`).

### The unification

**Wait-then-send-once, in every copy.** Better than spreading the retry on three
counts: it waits for the actual condition rather than guessing that three
attempts is enough; it removes the loop from user source instead of duplicating
it into six files; and it is the idiom ROS 2 users already know, which is the
whole point of these examples.

Ordered work:

1. **Bind the two C++ wrappers** — `Client::wait_for_service` and
   `ActionClient::wait_for_action_server`, thin over the existing C entry points
   (RFC-0019: the C header is the SSoT, C++ mirrors it). This is a real API gap
   independent of the examples: C exposes a blocking wait, C++ does not.
2. **Rewrite the six client examples** — native + nuttx × {c, cpp} ×
   {service, action} — to wait then send once, deleting the retry loops.
3. Delete the three divergence entries.

Runtime exposure to check before landing: native C/C++ service and action cells
are `Runtime`, and the nuttx C ones are too, so step 2 wants a real run on both
platforms rather than a build.

### Landed

All three steps, same day. **Divergences 4 → 1**, and the remaining one is the
PERMANENT native `c/listener` affordance — so every closable divergence in the
tree is now closed, and the phase is at its floor.

Two things the implementation turned up:

* **The examples touched were 15, not 6.** Only NuttX carried the retry loop,
  but converging means every copy of the program changes — freertos,
  riscv64-threadx and threadx-linux all matched OLD native, so they diverged the
  moment native moved. The gate named them immediately, which is the ratchet
  working: three programs × five group-A platforms.
* **The action shim must go through `ActionClientCore`'s public accessors**
  (`{start,poll}_server_discovery`, `is_server_ready`), not `send_goal_client`.
  Those exist for exactly this — their doc says "used by the C action-client
  wrapper … to keep `send_goal_client` private while still exposing the
  discovery surface". Reaching the field compiled under one feature set and
  failed under `check-cpp`'s.

Verified by running on native: the C action pair completed a full Fibonacci
round trip, `cpp/service-client` printed `Result of add_two_ints: 5`, and
`cpp/action-client` took three feedback publishes to a result. `just check c`
and `just check cpp` pass.

**Not verified**: the freertos / riscv64-threadx / threadx-linux copies are
source-converged and gate-checked but unbuilt here — their toolchains need the
per-platform `just` recipes. The nuttx C action cell is `Runtime`, so it wants a
real run before this is called proven on embedded.

## W2.d / W3.c / W7 — closed 2026-08-05

### W2.d — not an inconsistency; closed as invalid

The claim was that `#![no_std]`/`#![no_main]` is applied inconsistently "for the
same generated entry". Measured across every collapsed entry, it correlates
perfectly with the target triple:

| carries the attributes | does not |
|---|---|
| `thumbv7m-none-eabi` (freertos, mps2), `riscv64gc-…-none-elf`, `riscv32imc-…-none-elf` | host, `x86_64-unknown-linux-gnu` (threadx-linux), `armv7a-nuttx-eabihf` |

Every `*-none-*` target has no OS supplying `main`; the others do. It tracks the
runtime model, not drift, and it could not be unified anyway — `#![no_main]` on
a hosted target breaks the normal `main`, and inner attributes cannot be emitted
by a macro. It also affects nothing: `main.rs` is GLUE to the gate.

### W3.c — measured, and the answer removed the wave

W3.c asked for group B's irreducible delta "after unifying the logging facade".
The measurement made the unification beside the point: **B had one real member.**
`qemu-esp32-baremetal` was classified B on the assumption that bare-metal implies
deferred dispatch, but its bodies are plain immediate-dispatch group-A code —
talker byte-identical to group A, listener short one `log::info!` line — and its
Pubsub cell RUNS that way. Moved to A; both divergences deleted; group renamed
`B-deferred` because the axis is the dispatch strategy, not the presence of an
OS.

### W7 — done, in the opposite direction to the draft

W7.a's premise ("`log` needs std") is false: esp32 bridges `log` on `no_std` via
esp_println. And the weight was already on `log` — 6 of 7 boards bridge it,
exactly one board's bodies used `nros_log`. So instead of adding `nros_log` to
three boards and moving five bodies, W7 added ONE bridge and moved TWO bodies.

* `nros-board-mps2-an385` gains `install_semihosting_log_bridge`.
* Its talker and listener use `log::info!`; the `Logger` static and
  `register_logger` call leave user source.
* **Decision (W7.c): `log` is the user-facing facade; `nros_log` is the
  platform/ABI layer** — it backs `nros_platform_log_write` and therefore the C
  API. Not "secondary", *different layer*.
* W7.d: group B's reason is now dispatch-only.

Verified by running the Runtime cell, which is the whole risk W7.b names:
talker under QEMU printed `Publishing: 'Hello World: 1..10'`; the listener,
subscribed to a NATIVE talker through the same router, printed
`I heard: [Hello World: 34..46]`.

**Declared residue** — 11 `qemu-arm-baremetal` bodies (RTIC / serial / xrce) and
`workspaces/rust/src/talker_pkg` still use `nros_log`. The mps2 ones are
single-platform demos with no cross-platform twin, so no portability claim rests
on them. The workspace pkg is the reference shape and should follow; it was left
because its lane is not runnable on this host and W7.b's risk is precisely the
kind not to take unverified.

## W6 — CLOSED: prove portability with a CHECK, keep the copies (maintainer, 2026-08-05)

**Decision: do not fold. Assert instead.** Portability is demonstrated by a test
that the copies are identical, and copy-out is preserved by leaving the copies
where they are. W6.b already pre-authorized exactly this — *"if that cannot be
kept, do not fold; the gate alone already removes the drift risk, which was the
actual problem"* — so this closes the wave on its own terms rather than
abandoning it.

Why folding loses: the copies exist so a user can `cp -r` a leaf out and build
it (RFC-0026). Every way to fold either breaks that (`include!` and symlinks
make the copied directory non-standalone) or adds a templating surface nothing
else in the tree needs. **Duplication that cannot drift is not a defect — it is
the price of copy-out**, and W1 made it undriftable.

Measured at close: `rust/talker` exists in 9 platform directories and
`rust/listener` in 8; for group A, four of five copies are byte-identical in
code and the fifth differs only by `mod app_main;` (glue the gate excludes).
The sole on-disk difference is each file's doc header naming its platform.

### The two halves, both now gated

| property | gate | status |
|---|---|---|
| the copies are IDENTICAL (portability) | `example_portability::copies_within_a_group_are_identical` + the `no_stale_divergence_entries` ratchet | 6/6, 14 triples identical, 6 declared exceptions |
| a copy can be COPIED OUT and built | `example_shape::every_standalone_rust_leaf_is_its_own_workspace_root` | **new, 2026-08-05** |

The second one was the gap. Until now copy-out was asserted only as "every leaf
ships a README" — that the *instructions* travel, not that the thing still
builds once it lands elsewhere. The mechanism that makes it true is the empty
`[workspace]` table in each leaf manifest (standalone root + outer-workspace
adoption guard), and nothing checked for it. All 71 standalone leaves had it, so
this codifies a property that held by convention. Dropping it fails indirectly —
cargo adopts the leaf into whatever workspace it finds upward, which reads as a
dependency or feature-unification problem, not a missing table.

Membership is decided structurally (does an ancestor manifest declare
`[workspace]`?), not by path name — a first draft skipped anything under a
`workspaces/` component and false-flagged
`examples/templates/multi-node-workspace/src/*`, which is the same kind of
member living somewhere else.

### Reopen only if

A materializer gets built for another reason — if `nros new` ever templates
example projects, folding becomes nearly free and this is worth revisiting. Do
not build one *in order to* fold.


- [x] **W6.a** CLOSED — not folded, by decision above.
      ~~Fold the copies within each portability group to one canonical~~
      source plus per-platform build configuration (`Cargo.toml`,
      `.cargo/config.toml`, `package.xml`, `CMakeLists.txt`), which is where the
      real platform difference has always lived.
- [x] **W6.b** SATISFIED — its own escape clause is what closed the wave, and
      the copy-out property it names is now a test rather than a convention.
      **Preserve what the copies are for.** They exist so a user can copy
      one out (RFC-0026) and so `talker`/`listener` mirror the ROS 2 examples that
      make nano-ros legible to ROS users. Folding must keep a per-platform copy
      *materializable* — generated or templated — not force users into a
      workspace. If that cannot be kept, do not fold; the gate alone already
      removes the drift risk, which was the actual problem.
- [x] **W6.c** DONE — Zephyr is declared as group C in the gate, with its
      reason recorded there.
      **Zephyr stays an exception**, declared with its reason: its
      component shape (`Talker.c` / `.hpp`) is the convention Zephyr users expect,
      not a portability failure. Same for any other shape exception W1.b records.

## W7 — One logging facade in user source

**Filed 2026-08-04 from the W3 blocker; option (c) of the three recorded there.**
W3 took option (a) — `nros-board-linux` now bridges `log` alongside its
`nros_log` init — which unblocks the migration but leaves the underlying split
in place. This wave removes it.

### The split, measured

| board | inits `nros_log` sink | bridges `log` |
|---|---|---|
| posix / native | yes | **yes, since W3 (a)** |
| mps2-an385 (bare-metal) | yes | no |
| freertos | no | yes |
| nuttx | no | yes |
| threadx | no | yes |

The node bodies inherit it exactly: the four scheduled-platform standalone
copies use `log::info!` because their boards bridge `log`; bare-metal uses
`nros_log::nros_info!` because its board inits that sink. **The logging facade
is a board property leaking into user source** — the same defect class this
phase exists to remove, and it is why a Node body copied to native compiled,
ran and printed nothing until W3 (a).

It is also a live group boundary: `example_portability`'s group B declares "uses
the `nros_log` facade because `log` needs std" as one of its two reasons for
being a separate group. Removing the split removes one of them.

- [x] **W7.a** Add `nros_log::init(sinks::default())` to the freertos, nuttx and
      threadx boards, beside their existing `log` bridges, so both facades work
      everywhere before any body moves.

      **Surveyed 2026-08-05, then DISPROVED by measurement the same day.** The
      survey traced `sinks::default()` to the `nros_platform_log_write` C ABI,
      grepped for its providers, found none for nuttx and no
      `nros_platform_register_log_writer` caller for threadx, and concluded the
      facade was a silent no-op on both — filed as issue 0420 and used to mark
      W7.a blocked.

      **That was wrong, and it was wrong in an instructive way.** Issue 0420 is
      archived `resolved  # not-a-bug`: every row was disproved by *running* the
      cells rather than reading them.

      * **NuttX works.** `nm` shows `T nros_platform_log_write` in the image and
        a direct QEMU boot prints all six severities. The definition arrives by
        REUSE — `nros-board-common`'s `nuttx_platform_build.rs` compiles
        `nros-platform-posix/src/platform.c` against the board's headers. The
        survey searched for a *nuttx-specific* `platform.c`, so a definition that
        exists looked like a definition that does not.
      * **ThreadX works** (shown by running the threadx-linux fixture).
      * **FreeRTOS-via-Rust** registers its writer at
        `mps2-an385-freertos/src/lib.rs:111` — not only from the C entry.

      Implementing the "fix" this section previously prescribed would have added a
      SECOND log writer to three platforms that already had one.

      The lesson is the one this repo keeps re-learning: a grep that finds nothing
      is evidence about the grep, not about the tree. `nros_platform_log_write`
      reaches nuttx through a build script, which no amount of searching
      `nros-platform-nuttx/` will reveal. Boot the thing.

      **What IS real**, and is the finding 0420 half-saw: every NuttX cell SKIPS
      unless `NUTTX_DIR` is exported — `activate.sh` and the SDK env never set it,
      and configuring NuttX needs kconfig tooling nothing provisions. The cell
      reported SKIP on a host that had the sources, the toolchain and QEMU. That
      is the issue-0407 shape one layer further out, and it is why a facade
      regression on nuttx could sit unnoticed indefinitely. **W7.a is not blocked;
      W7's real prerequisite is that the nuttx cells can actually run.**

- [x] **W7.b** Move the node bodies to `nros_log::nros_info!`. **The asserted
      markers survive** — the format strings do not change, only the macro that
      emits them, so `TALKER_LOG_PREFIX` / `LISTENER_LOG_PREFIX` still match.
      Verify that claim per platform rather than assuming it; a facade that
      prefixes or reformats would break every e2e grep at once.
- [x] **W7.c** Retire the now-unused `log` bridges, or keep them and document
      `log` as a supported-but-secondary facade. Decide explicitly — leaving both
      undocumented is how the split happened.
- [x] **W7.d** Re-run the group-B reason in the gate: with the facade unified,
      B's remaining difference is the execution model alone
      (`DispatchStrategy::Deferred` + explicit `tick()`). Update the declared
      reason, and re-measure whether B still needs to be its own group.

**Sequencing:** after W3 completes. Doing it first would change three boards,
five bodies and the reference platform's examples in one step with the native
lane as the only witness.

**Risk:** every e2e that greps a talker/listener marker depends on the emitted
line surviving the facade change. W7.b is the whole risk of this wave and it is
verifiable per platform before landing.

---

## Sequencing

```
W1 (gate) ─► W2 (rust ceremony) ─► W3 (native split) ─► W6 (fold)
                                        └────────────► W7 (one log facade)
W4 (arch panic)     ─ independent, small; unblocks phase-337's industrial claim
W5 (board branches) ─ independent; W5.b orders phase-337 W7.b
```

W7 follows W3 rather than blocking it: W3 took option (a) (the posix board
bridges `log`), which unblocks the migration and leaves the two-stack split for
W7 to remove properly.

W1 first is the load-bearing choice: it is cheap, it stands alone, and without it
every later wave is unverifiable and every fold is a leap.

## Acceptance — all met (2026-08-06)

- [x] The gate exists and passes, with every exception carrying a written reason.
      **Stronger than asked: there are no exceptions left.** 18 of 18
      `(lang, program, group)` triples byte-identical, 0 divergence entries,
      down from 41 at baseline.
- [x] `rust/talker` and `rust/listener` are ONE body **per group**. Group A now
      spans SEVEN platforms (esp32 joined it — W3.c found it was never a group-B
      member), B is `qemu-arm-baremetal` alone, C is Zephyr alone.
- [x] ~~No example source contains `force_link_backend!`, `*_app_main!` or
      `extern crate <board> as _`~~ — **REWORDED, and the reason matters.** That
      criterion was written for option (a) (every platform gains an `-entry`
      package). The maintainer chose **option (b)**: node logic and boot glue
      live in separate FILES, and the glue keeps its ceremony because a
      staticlib target really does need an `app_main` symbol a hosted binary
      does not. The honest criterion is *no LOGIC file contains ceremony*, which
      `example_portability::ceremony_stays_out_of_node_logic` enforces and which
      passes. 13 glue files legitimately carry it.
- [x] A `thumbv7em-none-eabihf` FreeRTOS build does not panic — `[arch.cortex-m7]`
      admits it, and `arch_flags::freertos_lwip_resolves_both_declared_arches`
      asserts it by name ("the M7 blocker"). Verified as a TEST rather than a
      one-off build, so it stays true; confirmed reachable from the default
      sweep (`nextest -E 'test(freertos_lwip_resolves)'` selects it).
- [x] No cmake or build-script decision branches on a board *name* where a
      capability or platform is what it means (W5).
- [x] ONE logging facade appears in node source, and no board's choice of sink
      decides which one an example may use. **`log` everywhere: zero `src/lib.rs`
      node bodies use `nros_log`.** Completing this needed the bridge moved out
      of the `board-entry`-gated module into an ungated one, because the RTIC
      boot path could not reach it — an RTIC body written against `log::info!`
      would have compiled and printed NOTHING, which is precisely the silent
      failure W7.b names. Verified by booting the converted RTIC talker:
      `Publishing: 'Hello World: N'`. The 15 Application-shaped `main.rs` demos
      still use `nros_log` and should — one of them IS the `nros_log` demo, and
      `nros_log` remains the platform/ABI layer beneath `log`.
- [x] The book states "the same source runs on every supported target" with the
      gate as its citation (`book/src/introduction.md`, Key Features).

## Close-out (2026-08-06) — COMPLETE

The `**Status.** DRAFT — not started.` line near the top was **wrong** and stayed
wrong through the entire phase: all 25 items are done, the Acceptance section
below records every one as met, and `ab40ab25e` landed the last of them. Left
the original line in place rather than editing history out of the doc — a status
field that survived a whole phase without being touched is worth seeing, because
the same drift put four finished phases in the active roadmap and mislabelled two
issues in the same week.

Verified rather than assumed:

- **The gate runs and passes.** `nros-tests::example_portability
  report_portability_baseline` — the W1 guard that normalizes every platform copy
  and asserts byte-identity within a portability group.
- **The book claim exists and is cited.** `book/src/introduction.md:67` — "The
  same source runs on every supported target", with the gate as its citation,
  which is what makes the claim checkable rather than aspirational.

## Risks

- **Folding too early.** W6 before W1–W3 would delete copies that are not actually
  identical yet. The gate is the precondition, not the paperwork.
- **The `-entry` shape may not cover every example.** It is proven for
  talker/listener/service/action on three platforms; RTIC, embassy, serial and
  uORB variants have their own entry shapes and may stay exceptions. Measure
  before promising — RFC-0064's exploration log is explicit that inferred gaps do
  not survive contact.
- **W5.b is on phase-337's critical path.** `orin-spe` looks like a deletable
  scaffold and is not. Untangle before anyone deletes it.
- **This phase buys no CI time either.** It removes source copies and drift, not
  fixture rows. The honest pitch is maintainability plus a portability claim that
  is finally checkable.
