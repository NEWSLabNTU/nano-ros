# Phase 366 — The panic platform API, and one fatal path per image

**Status (2026-08-18).** COMPLETE — W1–W7 landed, M1–M6 and R1–R4 done.

The ending belongs to the image on every surface. A Rust entry writes
`nros::main!()` and gets the board's ending; a C/C++ entry writes
`nano_ros_entry(…)` and gets the same; an image bringing its own says `own`, and
the build can tell that from having forgotten. `panic-spin` is deleted, the
per-platform `panic-halt` table rows are gone, and `panic_to_platform!()` is the
documented escape hatch for entries no `main!()` expansion reaches.

**Carries a behaviour change, deliberately:** an embedded C/C++ image used to
halt silently on panic (the table said `panic-halt`, and nros-c's gate made halt
always win), so W4's forward was unreachable on exactly the images it was written
for. Those images now end the way their board ends. `PANIC halt` restores the old
behaviour per image.

The lang item now belongs to the image on all three boards. `nros-board-nuttx`,
`nros-board-threadx-qemu-riscv64` and `nros-board-mps2-an385-freertos` no longer
declare or import a `#[panic_handler]`; each keeps its BEHAVIOUR as a strong
`nros_platform_panic`, and ~23 images declare their own ending —
`nros::panic_to_platform!()` where the crate deps the facade, the same body
written out where it does not.

Two images deliberately keep a different provider and are left alone:
`logging-smoke-freertos-mps2` and `examples/workspaces/rust/src/freertos_entry`
use `panic-semihosting`, the smoke bin with `features = ["exit"]` so a panic ends
the QEMU run instead of hanging the harness. That is the design working, not an
exception to it.

Verified per family as each migrated: ThreadX-RV64, NuttX and FreeRTOS fixtures
all build, zero panic-handler errors, `check-archive-lang-items` green across 248
link lines.

**The full tier-2 sweep is currently blocked by an unrelated upstream red**:
`e228a8e80 feat(#626)` added an unguarded `sched_get_priority_min/max` to
`zpico.c`, which Zephyr's libc declares only behind `CONFIG_POSIX_API` — the same
family as the `pthread_t` break earlier in this phase. No zpico or zephyr file is
touched by this phase's commits.

Implements: RFC-0077. Tracked issues: 0618 (the design defect), 0617 (the two
failure modes it produces).

## Why

`<nros/platform.h>` exposes clock, **allocation**, atomics, sleep, yield,
random, wall clock, tasks, mutexes, condvars, wake, critical section and
logging — and nothing for "the world ended". So the allocator is a platform fact
that C and Rust share through one funnel, and panic is not expressible at all,
which is why each language invented its own ending and four libraries each
hardcoded a different one:

| provider | behaviour |
| --- | --- |
| `nros-c` | `loop { spin_loop() }` — silent |
| `nros-board-nuttx` | `println!("nros: PANIC {info}")`, `exit(1)` |
| `nros-board-threadx-qemu-riscv64` | UART `"PANIC: "`, exit QEMU |
| `nros-board-mps2-an385-freertos` | semihosting, `bkpt #0`, spin |

An image is one artifact. One fatal path is the only coherent answer.

## Work items

### W1 — the ABI entry point (gates everything else)

Add to `packages/platform/nros-platform-api/include/nros/platform.h`, in the
shape the allocation section already uses:

```c
/* ---- Fatal error ---- */
_Noreturn void nros_platform_panic(const char *msg, size_t len);
```

`msg` is a diagnostic, NOT a C string: `len` bytes, no NUL required, possibly
empty. A port must tolerate being called from any context — interrupt,
scheduler-locked, or before the kernel starts.

Three things move together, because the header is the SSoT (RFC-0054):

- `scripts/gen-abi-bindings.sh` regenerated, `generated.rs` committed —
  `check-abi-bindings` fails on a diff.
- `PlatformPanic` trait in `nros-platform-api`, mirroring `PlatformAlloc`.
- `nros_platform_export_panic!` in `nros-platform-cffi`, so the three Rust ports
  can export it. RFC-0076 D4 records that the macro's symbol list is
  hand-maintained and drifted once already (W10 added task probes to the header
  and all five C ports but not the macro); this phase does not fix that, but it
  must not add a sixth instance of it.

**Acceptance:** `just check abi-bindings` green; every port still links.

### W2 — the five C ports

`nros-platform-{posix,zephyr,threadx,freertos,esp-idf}/src/platform.c`, each
mapping to the native fatal path it already has:

| port | mapping |
| --- | --- |
| posix | `fwrite` to stderr, `abort()` |
| zephyr | message via the log ABI, then `k_panic()` |
| threadx | message, then `tx_thread_terminate` of self / spin |
| freertos | the existing `freertos_hooks.c` body — message, `bkpt`, halt |
| esp-idf | `esp_system_abort()`, which honours `CONFIG_ESP_SYSTEM_PANIC_*` |

Weak where the toolchain supports it, so a C/C++ image can override with a
strong definition. Note posix/threadx-linux are hosted: they get `std`, so the
Rust side never reaches the ABI there — the C surface still needs it.

**Acceptance:** each port compiles; `just build-test-fixtures lane=tier2` builds
all eight families.

### W3 — the Rust ports

`nros-platform-mps2-an385`, `nros-platform-esp32-qemu`, and `nros-board-cffi`
via `nros_platform_export_panic!`.

### W4 — Rust panics reach the ABI

The default `#[panic_handler]` formats `PanicInfo` into a stack buffer and calls
`nros_platform_panic`. It moves OUT of `nros-c` (a library) — RFC-0077's rule is
that the entry owns it — but that move is W5's, so W4 lands the forwarding body
where the handler currently lives and proves the funnel works end to end.

**Acceptance:** a panic in a fixture prints through the port's path, not
`nros-c`'s silent spin.

> **NOT MET on C/C++ images, and W7 is what fixes it (measured 2026-08-17).**
> The forwarding body is gated `all(panic-spin, not(std), not(panic-halt))`, and
> `nros_feature_set()` appends `panic-halt` on every embedded C/C++ coordinate
> (freertos, esp-idf, threadx-rv64, generic cross) while `platform-*` supplies
> `panic-spin`. So `panic-halt` ALWAYS wins there: W4 is live on pure-Rust and
> dead on exactly the images it was written for, which is why the acceptance
> reads as satisfied — someone checked a Rust fixture.
>
> This is the same fact W7 records as its behaviour change ("a C/C++ embedded
> image halts on panic today"), stated from the other end: it is not merely that
> those images halt, it is that a work item already marked landed does not reach
> them. Verified by building `qemu-riscv64-threadx/c/listener` and reading the
> ELF — one `rust_begin_unwind`, and with the table's `panic-halt` removed
> `nros_platform_panic` resolves into the image where it otherwise does not.
>
> No action here: retiring the table is W7.b and the flip is W7.c/M6. Recorded so
> the W4 row is not read as covering both languages.

### W5 — migrate the providers, then move ownership to the entry

The order is FORCED, and the reason is issue 0617 from the opposite direction:
delete a provider before something else supplies one and every pure-Rust image
on that board fails with `#[panic_handler] function required`. Each step must
leave every image with exactly one.

Providers as they stand (verified in tree, not recalled):

| provider | shape | state |
| --- | --- | --- |
| `nros-c/src/lib.rs:160` | `#[panic_handler]` | **forwards** to the ABI (W4) |
| `nros-board-nuttx` | `#[panic_handler]` | **forwards**; behaviour is its strong `nros_platform_panic` |
| `nros-board-threadx-qemu-riscv64` | `#[panic_handler]` | **forwards**; ditto |
| `nros-board-mps2-an385-freertos` | `use panic_semihosting as _` | **not migrated** |
| `nros-c` / `nros-cpp` `panic-halt` | `use panic_halt as _` | feature-gated, library-owned |

**W5.a (done)** — threadx-qemu-riscv64 and nuttx export their behaviour as a
strong `nros_platform_panic`; their lang items only forward. The ungated one
moved first because it was the provider no feature could turn off.

**W5.b — the third board. BLOCKED on W5.c, and the attempt proved why.**

`nros-board-mps2-an385-freertos` imports the lang item from `panic-semihosting`
rather than writing one. Replacing that with a forwarding handler — the same
migration the other two boards took — fails:

```
error[E0152]: duplicate lang item in crate `panic_semihosting`
  (which `logging_smoke_freertos_mps2` depends on): `panic_impl`
```

The old arrangement worked only because BOTH sides named the same crate, so
cargo unified them into one lang item. The moment the board writes its own, the
image's `panic-semihosting` is a second definition.

The images split, and that is the whole constraint:

- `packages/testing/nros-tests/bins/logging-smoke-freertos-mps2` declares its own
  provider with `features = ["exit"]`, so a panic EXITS QEMU instead of hanging
  the harness. That is a deliberate image-specific policy and exactly what
  RFC-0077 says should happen — this image already owns its ending, for a
  reason the board could not know.
- the six `examples/qemu-arm-freertos/rust/*` declare NOTHING and depend on the
  board for theirs.

So the board cannot stop providing until those six have their own, and it cannot
start writing one while the smoke bin has its own. Both directions are blocked
by the same missing piece: **W5.c**. Attempted, reverted, recorded — the
alternative was a suppression feature that W5.c would delete again.

Note `examples/qemu-arm-baremetal/rust/*` already write `use panic_semihosting
as _;` in their own `main.rs`, which is a second existence proof of the target
shape alongside ESP32's `esp-backtrace`.

**W5.c — the entry emits a provider.** `nros::main!` and `nano_ros_entry()`
today emit NONE — verified, zero matches in `main_macro.rs`. This is the
substantive step, and W5.b's failed attempt shows it is a HARD prerequisite
rather than merely the next item. Default forwards to
`nros_platform_panic`; an entry that declares its own (or `use panic_halt as _`)
must win, which means the emitted default has to be suppressible rather than
unconditional.

**W5.d — delete the forwarding handlers**, in `nros-c` and the three boards,
once W5.c supplies one. Only now is it safe.

**W5.e — retire the features.** `panic-spin` disappears from `nros-c`/`nros-cpp`;
`panic-halt` stops being a library feature and becomes what it always should
have been — a dependency the IMAGE names.

> **REVISED 2026-08-17 — BLOCKED, and the sentence above is a Rust-entry answer.**
> A C/C++ image has no Rust crate to name a dependency in, and
> `nros-c`/`nros-cpp` are `crate-type = ["staticlib", …]`, so rustc requires the
> lang item WHEN THE ARCHIVE IS COMPILED — before cmake links anything.
> `panic-spin` is the only provider such a build has today, so deleting it as
> written leaves the archive with no handler, which W6's per-link-line gate
> cannot see. The replacement surface is W7 below; W5.e resumes as R1 of
> RFC-0077's retirement list once W7.M4 has landed.

**W5.f — amend `ARCHITECTURE.md` §2.** Panic's selector is the image, not
`platform-<rtos>`. The allocator's sentence stays: its implementation IS
platform-keyed and its arena must remain shared. Last, so the text lands only
once the code makes it true.

**Prerequisite for W5.d, not a nicety.** W6's gate counts per LINK LINE, which
catches duplication and cannot catch ABSENCE — and absence is precisely what
W5.d risks. Extending it to count per image COORDINATE should land first.

---

## W7 — the image says it in its own vocabulary (RFC-0077, decided 2026-08-17)

Review found two places where W5 assumed the image is a Rust entry crate. Both
are accepted; the invariant is unchanged and the surface grows a second half.

**W7.a — `main!()` carries the default.** `nros::main!(panic = "platform" |
"halt" | "own")`, so a Rust entry gets a working ending by saying nothing.
Today's mandatory `nros::panic_to_platform!()` beside `main!()` is a second
obligatory line no other `no_std` crate asks for, and forgetting it fails as a
missing lang item that names nothing in this framework. The RFC's original
objection — that emitting from `main!()` would collide with images declaring
their own — holds only for an UNCONDITIONAL emit; `own` is the opt-out that
makes it safe, and makes "deliberate" distinguishable from "forgot".

**W7.b — `nano_ros_entry(… PANIC platform|halt|own)`.** The same three values on
the cmake surface, lowering to ONE cargo feature on the staticlib build
(`panic-platform` / `panic-halt` / neither). This replaces
`cmake/NanoRosFeatureSet.cmake`'s hardcoded `panic-halt` at lines 120, 126, 140
and 147 — a library decision made on the image's behalf by a table its author
never sees, which is this phase's own defect one language over.

**W7.a is LANDED (M1, Rust half).** `nros::main!(panic = "platform" | "halt" |
"own")`, defaulting to `own`, so behaviour is identical to today. The emit is
gated `target_os = "none"` — libstd owns the lang item on hosted targets, so
without that gate M5's flip breaks every native example at once. `halt` needed a
body: `nros::panic_halt!()` joins `panic_to_platform!()` in the facade (mask
interrupts through the platform critical section, then spin) rather than making
an entry take a `panic-halt` dependency to spell one word in a macro it already
calls.

**W7.a-bis — placement is DERIVED, not chosen (RFC-0077 amendment 2026-08-18).**
`main!()` expands in the bin target, but six examples produce two final artifacts
from one crate, and the staticlib's lang item must come from the lib. The rule
covering every family is one sentence — *the entry macro of a final artifact
emits that artifact's handler* — and the dual-artifact case is resolved by
`main!()` reading `[lib] crate-type` from the manifest it already parses: a
package that produces a `staticlib` gets nothing from `main!()`, because the lib
owns it for both artifacts.

Declaring those six `panic = "own"` instead was considered and rejected: it would
overload `own` to mean both "I bring my own provider" and "my provider is in the
other artifact", which destroys the deliberate-vs-forgot distinction `own` exists
for, and asks an example author to know a Rust linkage rule to answer a question
about panics. Numbers, since an earlier count in this phase was wrong: 21 images
invoke `panic_to_platform!()`, of which 6 are staticlib-shaped — not the 13-of-28
first reported, which came from grepping manifests for the WORD `staticlib` and
matching a comment about a crate that had stopped being one.

**Two cleanups this made visible, both landed.** The board's C-ABI entry macro
`cyclonedds_app_main!()` is now `app_main!()`: its body is
`run_app_thread($register)`, nothing in it is CycloneDDS, and an entry macro
named for a backend contradicts the RMW-portability promise. Cyclone is merely
the one backend whose embedded build must be CMake-linked — a build-system fact,
tracked as issue 0666. And `examples/threadx-linux/rust/*` dropped a
`crate-type` `staticlib` that nothing consumed (no CMakeLists, no C runtime, no
fixture row naming a `.a`), the same removal phase-359 W7 made on qemu-arm-nuttx.

**W7.b is UNBLOCKED, and the answer is the deferred one (RFC-0077 amendment
2026-08-18b).** M4's open question was where `PANIC` is resolved: a pre-import
global, matching how `BOARD`/`PLATFORM`/`RMW` reach `nros_feature_set()`, or a
feature set computed after entries are declared.

Reframing settles it. The question a build must answer is not "which macro is
this?" but WHO LINKS THE FINAL IMAGE — and the per-platform table cannot answer
it, because "does anything else already supply the handler?" is a property of the
entry and its link step. Zephyr proves it: `zephyr-lang-rust` links our
`rustapp` into its own ELF and the `zephyr` crate supplies the handler, so a
Zephyr-hosted target must be able to say `PANIC own` and mean it, while a bare
C/C++ image linking `libnros_c.a` has nothing and must emit. Same platform table,
opposite answers. So the staticlib's feature set has to be computable after
entries are declared.

**W7.c — migration, M1-M6 of RFC-0077.** Ordered so no commit leaves an image
with two providers or none: add the argument defaulting to `own` (behaviour
identical to today) → migrate the ~23 images that call `panic_to_platform!()`,
each in ONE commit that removes the call and adds the argument together →
declare the three images that bring their own (`esp-backtrace`,
`panic-semihosting`) → migrate the C/C++ entries → only then flip both defaults
to `platform`.

**W7.d — extend the gate to ABSENCE.** Count per image COORDINATE, not per link
line. Already named above as a prerequisite for W5.d; W7 is what makes it
load-bearing, because `own` is a promise the build must be able to check.

**Acceptance.** A new Rust entry writing only `nros::main!()`, and a new C/C++
entry writing only `nano_ros_entry(…)`, both link and panic through
`nros_platform_panic`. An image that supplies its own provider without saying
`own` fails with a message naming `panic = "own"` / `PANIC own`, not the lang
item. An image that says `own` and supplies nothing fails at the coordinate gate
rather than at the linker. `grep -rn "panic-spin" packages/ cmake/` is empty.

**R2 had a second route nobody had counted, and issue 0369 found it.** The
rename reached `nano_ros_entry(PANIC ...)`, which appends the feature to an
already-imported nros-c/nros-cpp with `corrosion_set_features()`. In a
workspace the linked archive is not that import: it is the synthesized
`nros_ws_runtime` umbrella, whose nros-cpp dependency says `default-features =
false`, so the crate default never applies and nothing appended the entry's
choice. The entry's generated header expected `panic-platform`, the umbrella's
nros-cpp was built without it, and the two met at the link as the variant
anchor's `undefined symbol:
nros_cpp_config_variant_alloc_panic_platform_..._std` — the anchor working as
designed, catching a feature split instead of shipping a mismatched ABI. Fixed
by reading `NROS_ENTRY_PANIC_POLICY` in `nros_synth_runtime_umbrella` (it runs
after the SUBDIRS loop, so every entry has already recorded its policy) and
baking the feature into the dep line, with the policy→feature mapping factored
into `nros_panic_policy_feature()` so the two routes share one spelling. Zephyr
was already consistent — that lane passes the SAME feature string to nros-cpp
and to the umbrella and selects no panic feature at all.

**Carries a behaviour change, and it is not silent.** A C/C++ embedded image
halts on panic today because the table says `panic-halt`; under `PANIC platform`
it ends the way its board ends. That is the intended ending, but it changes what
a shipped image does, so it is called out in RFC-0077's migration notes rather
than arriving as a default flip.

### W7.d — the Zephyr lane lowers PANIC itself (2026-08-19, handed off)

W7.b said the staticlib's feature set "has to be computable after entries are
declared". On the Zephyr lane it is not, and cannot be: a Zephyr app calls
`find_package(Zephyr)` — which runs `zephyr/CMakeLists.txt` and every
`nros_cargo_build()` in it — BEFORE its `nano_ros_add_executable()`. So six C/C++
workspace entries declaring `PANIC platform` died at configure:

```
nano_ros_entry(zephyr_entry): PANIC platform cannot be applied — no
nros-c/nros-cpp Rust target exists at this point in the configure.
```

Not an ordering slip but a MECHANISM mismatch. M4 applies PANIC with
`corrosion_set_features()` on an imported crate target; the Zephyr lane's Rust
side is `nros_cargo_build()`, which makes its own `add_custom_target` plus an
IMPORTED library named `nros_c_cargo` — not a Corrosion target, and not named
`nros_c`. The scan could never have matched, on any ordering.

**Fix.** The Zephyr module resolves the policy itself and appends the feature to
every `nros_cargo_build` FEATURES string, using the seam that lane already has
for entry->module facts: a variable set before `find_package(Zephyr)`, as
`NROS_WS_RUST_NODE_DIRS` is. Unset means `platform` (M5's default), so entries
saying `PANIC platform` needed no edit. `nano_ros_entry()` then VERIFIES
agreement against a global property instead of scanning — a contradiction is a
FATAL_ERROR naming both endings, so the keyword stays the declaration a reader
can trust rather than becoming decoration.

**The first attempt was wrong in a way worth recording.** The computation went
inside `if(CONFIG_NROS_C_API)`, and a C++-only image sets `CONFIG_NROS_CPP_API`
with `CONFIG_NROS_C_API` UNSET — so it silently did not happen and all six
entries still failed. It is hoisted above both branches now. A knob resolved
inside one API branch is invisible to the other, which is the same shape as this
phase's own "six providers, five gating idioms".

**Verified**, not assumed: `just zephyr build-fixtures` exits 0 (was 2), zero
PANIC errors, and the feature is on the real cargo command line —

```
--features rmw-cffi,cffi-zenoh-cffi,platform-zephyr,ros-humble,std,panic-platform
```

— and in the built crate's fingerprint, so the ending is compiled in rather than
silently skipped. Mutation-checked: an entry saying `PANIC halt` against a module
that built `platform` fails with both endings named.

`PANIC own` remains available on this lane and is what a Rust-on-Zephyr image
wants (W7.b: `zephyr-lang-rust` links our `rustapp` and the `zephyr` crate
supplies the handler); it maps to an empty feature, so nothing is selected.

### W6 — the gate

Extend `scripts/check-archive-lang-items.sh` to the panic lang item
(`rust_begin_unwind`), and count per image COORDINATE rather than per link line:
per-link-line catches duplicates but cannot catch ABSENCE, which is what 0617's
`#[panic_handler] function required` was.

**Acceptance:** the gate fails a deliberately-broken image both ways — two
providers, and none.

## Deliberately out of scope

- **The allocator.** Already per-image via 0616 and enforced by
  `check-archive-lang-items`; its arena must stay `nros_platform_alloc` and this
  phase must not offer a knob for it.
- **RFC-0076 D4's macro drift.** Real, adjacent, and its own work.
- **Per-RTOS fatal configuration** (`CONFIG_ESP_SYSTEM_PANIC_*`, Zephyr assert
  levels, NuttX crashdump). The ABI makes them reachable; choosing them is the
  image's and belongs with the `PANIC` entry option, not here.

## Risks

- **The ABI mirror.** A header edit without regenerating bindings is an
  immediate red (`check-abi-bindings`). Same commit, always.
- **Lang-item windows.** W5 can leave an image with zero or two providers
  between steps. W6's gate should land before W5's flips so the failure is a
  gate message, not a four-crates-away link error.
- **Host lanes cannot see any of this.** `std` supplies the handler, so every
  regression here surfaces only on embedded coordinates — tier 2 minimum.
