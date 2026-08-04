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

- [ ] **W1.a** A guard test that, for each (language, program), normalizes every
      platform copy (strip comments and blank lines, collapse whitespace) and
      asserts the copies within a **portability group** are byte-identical.
      Source comparison only — no fixture, no boot, no QEMU. Belongs in
      phase-329's guards bucket.
- [ ] **W1.b** **Exceptions are DATA with a reason, and they do not escape the
      gate — they form their own group.** Two kinds:
      - **Shape exceptions** — Zephyr's component convention (`Talker.c`,
        `Talker.hpp`, 34 loc against C's 89). Zephyr users expect that shape;
        this is a permanent, declared exception. The gate still asserts every
        Zephyr copy is identical *to the other Zephyr copies*.
      - **Affordance exceptions** — native's `NROS_SUB_TYPE` hook. Declared with
        the test that needs it, so deleting the test deletes the exception.
      An undeclared divergence is a failure. A declared one is a row with a
      reason — the same rule `Tier::CarveOut` already follows.
- [ ] **W1.c** Record the baseline table above as the gate's starting state, so
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
> **Open design question, for the maintainer — do not decide this unilaterally.**
> Either (a) every platform gains a committed `-entry` package (uniform, but more
> directories), or (b) the entry glue is a declared per-platform file inside the
> same package (fewer directories, and the gate's subject becomes the logic file).
> (b) fits W6's direction; (a) fits the existing three platforms. W2's remaining
> items are written for whichever wins.

The ceremony moves into the generated entry, where `nros::init`, executor open,
RMW registration and the spin loop already live.

- [ ] **W2.a** Teach the generated entry to emit what the examples currently
      hand-write: `force_link_backend!` for the selected RMW(s), the board's
      `*_app_main!` / `zephyr_component_main!` invocation, and the
      `extern crate <board> as _` / `extern crate alloc` link anchors. The
      generator already knows the board (`board_path_for`), so this is emission,
      not new resolution.
- [ ] **W2.b** Migrate the plain `rust/{talker,listener,service-*,action-*}`
      examples to the two-file `-entry` shape.
- [ ] **W2.c** Normalize the node-package names so the `pub use <node>::register;`
      line is identical across platforms (today `freertos_rs_talker` vs
      `nuttx_rs_talker`). Naming rules already exist in
      `examples/workspaces/README-layout.md`; extend them to examples.
- [ ] **W2.d** Normalize the `#![no_std]` / `#![no_main]` inconsistency — the
      freertos `talker-entry/src/main.rs` carries both attributes while the nuttx
      and threadx-linux ones carry neither, for the same generated entry.
      *Result:* threadx-riscv64 and zephyr join the RTOS group;
      `rust/talker` goes from 4 bodies to 2 (RTOS+baremetal, and native).

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
   `log::info!`, and on native neither `nros-board-native` nor `nros-board-posix`
   installs a `log` sink; the imperative example called `env_logger::init()`
   itself, and a Node body has nowhere to put that because `nros::main!()` owns
   `main`. Every test asserting `TALKER_LOG_PREFIX` would go silent.

   Note the facades already disagree, which the gate could not see because
   native was not comparable: the four group-A standalone copies use
   `log::info!`, while `workspaces/rust/src/talker_pkg` uses
   `nros_log::nros_info!(&DEFAULT_LOGGER, …)` — and *that* one works on native
   with no init, which is why the workspace fixture is green today.

   **Three ways out, and this is a product decision:**
   (a) `nros-board-native`'s `BoardEntry::run` installs the hosted logger, which
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

**This is a runtime defect, not a migration problem, and it needs its own issue.**
Node-class actions *do* work on native: the matrix carries four `Native` ×
`Action` × `Workspace` Runtime cells, which run through
`nros::main!(launch = …)`. The standalone path differs only in its spin:
`__nros_hosted_spin_forever` loops on `runtime.runtime.spin_once(10)` alone,
while the RTIC path's own comment describes the correct pattern as
"`spin_once(ms)` + `run_ticks`, matching the owned-spin boards, so **service/
action poll components tick**" (`main_macro.rs:2140`). A tick-driven node —
action server, service client — therefore cannot run under
`nros::main!(spin = "forever")`.

That also predicts `service-client` will fail the same way (its group body is
tick-driven too), and explains why `talker` and `service-server` migrated
cleanly: neither uses `tick()`.

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

- [ ] **W3.a** Establish why the hosted Rust example is 91 lines when its embedded
      sibling is 34 — how much is genuinely hosted-only (arg parsing, signal
      handling, `std` logging) versus ceremony the generated entry could own on
      both sides. **ANSWERED above: ~2/3 is the triple-implemented type switch.**
- [ ] **W3.b** Bring native onto the entry shape so the body is the same 34 lines,
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

- [ ] **W3.c** Measure B's irreducible delta after unifying the logging facade and
      defaulting `DISPATCH`/`tick`. Record the remainder as group B's declared
      reason. **Do not force B into A** — an execution-model difference expressed
      as one body with cfg branches is worse than two honest bodies.

## W4 — Arch portability: the FreeRTOS Cortex-M4F/M7 panic

**Owner moved here from phase-337 W1.a** — it is a source-portability defect, so
it belongs with the rest of them. phase-337 keeps a pointer.

- [ ] **W4.a** `packages/boards/nros-board-freertos/build.rs:273-287` **hard-panics**
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

## W6 — Fold the per-platform copies (gated on W1–W3 green)

Only once the gate proves the bodies are identical. The point of the earlier
waves is that by here, folding destroys nothing.

- [ ] **W6.a** Fold the copies within each portability group to one canonical
      source plus per-platform build configuration (`Cargo.toml`,
      `.cargo/config.toml`, `package.xml`, `CMakeLists.txt`), which is where the
      real platform difference has always lived.
- [ ] **W6.b** **Preserve what the copies are for.** They exist so a user can copy
      one out (RFC-0026) and so `talker`/`listener` mirror the ROS 2 examples that
      make nano-ros legible to ROS users. Folding must keep a per-platform copy
      *materializable* — generated or templated — not force users into a
      workspace. If that cannot be kept, do not fold; the gate alone already
      removes the drift risk, which was the actual problem.
- [ ] **W6.c** **Zephyr stays an exception**, declared with its reason: its
      component shape (`Talker.c` / `.hpp`) is the convention Zephyr users expect,
      not a portability failure. Same for any other shape exception W1.b records.

## W7 — One logging facade in user source

**Filed 2026-08-04 from the W3 blocker; option (c) of the three recorded there.**
W3 took option (a) — `nros-board-posix` now bridges `log` alongside its
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

- [ ] **W7.a** Add `nros_log::init(sinks::default())` to the freertos, nuttx and
      threadx boards, beside their existing `log` bridges, so both facades work
      everywhere before any body moves.
- [ ] **W7.b** Move the node bodies to `nros_log::nros_info!`. **The asserted
      markers survive** — the format strings do not change, only the macro that
      emits them, so `TALKER_LOG_PREFIX` / `LISTENER_LOG_PREFIX` still match.
      Verify that claim per platform rather than assuming it; a facade that
      prefixes or reformats would break every e2e grep at once.
- [ ] **W7.c** Retire the now-unused `log` bridges, or keep them and document
      `log` as a supported-but-secondary facade. Decide explicitly — leaving both
      undocumented is how the split happened.
- [ ] **W7.d** Re-run the group-B reason in the gate: with the facade unified,
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

## Acceptance

- [ ] The gate exists and passes, with every exception carrying a written reason.
- [ ] `rust/talker` and `rust/listener` are ONE body **per group** — A, B and C —
      where today group A alone is split across 4 bodies plus 2 near-misses.
      Three groups is the honest target; one is not.
- [ ] No example source contains `force_link_backend!`, `*_app_main!` or
      `extern crate <board> as _` — the generated entry owns all of it.
- [ ] A `thumbv7em-none-eabihf` FreeRTOS build does not panic.
- [ ] No cmake or build-script decision branches on a board *name* where a
      capability or platform is what it means.
- [ ] ONE logging facade appears in node source, and no board's choice of sink
      decides which one an example may use (W7).
- [ ] The book can state "the same source runs on every supported target" with the
      gate as its citation.

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
