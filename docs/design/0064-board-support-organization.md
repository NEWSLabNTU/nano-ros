---
rfc: 0064
title: "Board support organization: nano-ros as an embeddable library, not a board framework"
status: Stable
area: platform-board-rtos
since: 2026-07
last-reviewed: 2026-11-14
implements-tracked-by: [phase-337, phase-375, phase-215]  # R3's matrix; 375 = tier policy + onboarding cost + R5's process; 215 = the FVP migration
amends: [RFC-0012]  # "finishes it and removes the parts that still contradict it"
supersedes: []
superseded-by: null
---

# RFC-0064 — Board support organization

> **Stable (2026-11-14, phase-555 W3, user ruling P7 (b)).** This RFC was a
> live exploration from 2026-07-31 to 2026-09-06; all its phases are archived
> (phase-337, phase-375, phase-215). It is now split in two:
>
> - **This file** is the design of record: the stance (Summary, the seam,
>   where board support survives), Revision 5 (one board process, primitives
>   only), and the open questions, which are filed as issues.
> - **[The exploration record](archived/board-support-organization-exploration-log.md)**
>   (archived companion) keeps the worked example, Revision 3's supported
>   matrix, the defects found, the sequencing, the dated exploration log and
>   Revision 4's tier policy.
>
> Superseded in part: the tier definitions (Revision 3's matrix, Revision 4's
> policy) by [RFC-0114](0114-board-support-registry-primitives.md) D10/D11,
> board admission to the book by
> [RFC-0115](0115-emulator-first-board-policy.md), and the book rows (the
> porting-guide row, the tier page) by
> [RFC-0124](0124-book-user-guide-organization.md). This RFC amends
> [RFC-0012](0012-board-bsp-integration-architecture.md): it finishes 0012's
> stance and removes the parts of the tree that still contradicted it.
>
> (Renumbered 0062 → 0064 on 2026-08-01 after an id collision with
> unified-dependency-ssot; 0064 is the settled id.)

## Summary

**nano-ros should be an embeddable library plus the build system to compile a
ROS-like project against it. Board support is not a nano-ros concept.**

The host ecosystem — Zephyr, ESP-IDF, NuttX, a vendor SDK, PlatformIO — already
owns boards, drivers, pin mux, linker scripts and DTS. nano-ros competing with
that produces one crate per `(vendor × board × SDK-variant)`, which does not
scale and which RFC-0012 already rejected in 2026-05.

What nano-ros owns is narrow and stable:

1. a **portability seam** — the `nros_platform_*` C ABI,
2. a **capability declaration** — which link features it wants, not which stacks exist,
3. a **build system** — codegen, entry generation, `NanoRos::NanoRos`,
4. a **≤200-line integration shell** per host ecosystem.

Everything else is the integrator's, and nano-ros's job is to stay out of the
way and be discoverable from outside its own tree.

## The stance, and the evidence it already works

`integrations/nano-ros/CMakeLists.txt` **was** the whole ESP-IDF story — **146
lines**, under Phase 139's documented ≤200 LoC hard cap per shell. phase-468 W2
(`083d2c10d`) deleted it along with the platform crate and the cmake file below;
it is quoted here as the EVIDENCE this RFC's stance was workable, not as
something a reader can go and look at. It did four things:

```cmake
set(NROS_PLATFORM "esp_idf" CACHE STRING "" FORCE)   # 1. name the platform
# ... map CONFIG_NROS_* (Kconfig) → nano-ros cache vars   # 2. adopt host config
add_subdirectory("${_nros_root}" nano_ros_root)           # 3. pull in the build system
target_link_libraries(${COMPONENT_LIB} INTERFACE NanoRos::NanoRos)  # 4. re-export
```

Plus, in `cmake/platform/nano-ros-esp_idf.cmake` (also deleted in phase-468 W2),
the only board-ish work there ever was:

```cmake
add_library(freertos_kernel INTERFACE)
target_link_libraries(freertos_kernel INTERFACE idf::freertos)
add_library(lwip INTERFACE)
target_link_libraries(lwip INTERFACE idf::lwip)
```

That is **every ESP32 part supported**, with no board crate, no board module, no
`NROS_BOARD`, and no descriptor. The module comment says so outright: *"No
NROS_BOARD requirement — IDF supplies every artefact the FreeRTOS board
overlays would have shipped."*

The same shape already exists for NuttX (`integrations/nuttx/` — Kconfig, `Make.defs`
and a `Makefile` for NuttX's make build; its CMake entry was deleted by
RFC-0126 decision 16), PX4, PlatformIO, and
Zephyr (nano-ros ships `zephyr/module.yml` declaring `build.cmake`,
`build.kconfig`, `snippet_root`, and Twister `samples`). Four ecosystems, all
integrated, none of them enumerated dimension by dimension.

**This is the model. It is already the Stable design of record** — RFC-0012,
"Board / BSP Integration Architecture", drafted 2026-05 *"in response to 'vendor
BSPs vary; we can't ship a board crate per (vendor × board × SDK-variant)
combo.'"* Its Layer 4 is labelled **"Vendor BSP — owned BY the vendor, NOT by
nano-ros."** RFC-0064 does not invent this; it finishes it and removes the parts
of the tree that still contradict it.

## What nano-ros needs (the seam)

### The ABI: 92 C functions

`packages/platform/nros-platform-api/include/nros/` declares the entire
portability contract as `nros_platform_*`. Grouped:

| Group | Count (approx) | Examples |
|---|---|---|
| clock / time | 7 | `clock_us`, `clock_ms`, `time_since_epoch_nanos` |
| sleep / yield | 4 | `sleep_ms`, `yield_now` |
| tasks | 6 | `task_init`, `task_join`, `task_detach` |
| sync | 20 | `mutex_*`, `mutex_rec_*`, `condvar_*`, `critical_section_*` |
| wake | 7 | `wake_init`, `wake_signal_from_isr`, `wake_wait_ms` |
| alloc / heap | 5 | `alloc`, `free`, `heap_used_bytes` |
| random | 5 | `random_u32`, `random_fill` |
| log | 3 | `log_write`, `register_log_writer` |
| **sockets / net** | **~33** | `tcp_*`, `udp_*`, `udp_mcast_*`, `socket_*`, `network_poll` |
| misc | 2 | `atomic_load_bool`, `zephyr_wait_network` |

Satisfy these and nano-ros runs. That is the honest definition of "embeddable
library", and it is a real boundary, not a slogan — the ESP-IDF, NuttX and Zephyr
shells all cross it without either side knowing much about the other.

**[OPEN, issue 2539]** How much of the 92 is *mandatory*? A serial-only, single-threaded,
no-heap deployment plainly does not need 33 socket functions. If the mandatory
core is ~25 and the rest is capability-gated, "port nano-ros to my RTOS" becomes
a weekend, and that number belongs in the porting guide's first paragraph. Worth
measuring: build a minimal platform impl and see what the linker actually demands.

### The capabilities: link features, not stacks

Revision 1 proposed enumerating net stacks (`lwip | smoltcp | netx | rtos |
host`). **That was the wrong shape** — it names implementations, so the list
grows with every ecosystem nano-ros meets, and a user whose RTOS has a network
stack nobody has heard of is simply unrepresentable.

nano-ros already has the right shape and has had it since Phase 134.
`nros-board-common/src/policy.rs`:

```rust
pub struct LinkFeatures {
    pub tcp: bool, pub udp_unicast: bool, pub udp_multicast: bool,
    pub serial: bool, pub raweth: bool, pub tls: bool,
    pub ivc: bool,          // Phase 100.4 — NVIDIA Tegra IVC
    pub custom: bool,       // Phase 115.B — runtime-pluggable user transport
}
```

paired with `LinkPolicy`, a per-platform mask using `Force(false)` / `Follow` —
*"Orin SPE has no Ethernet → `Force(false)` masks TCP / UDP / MC / SERIAL / TLS."*

This is capability negotiation: nano-ros says *which transports it wants*, the
platform says *which it can provide*, and the intersection decides what gets
compiled and which socket functions must exist. nano-ros never learns whether
the bytes leave through lwIP, NetX Duo, smoltcp, a Zephyr driver, or a vendor
DMA ring. **`custom` is the escape hatch that makes the set closed rather than
open-ended** — an RTOS with an exotic link implements one transport vtable and
stops there.

So: **delete `net_stack` from the descriptor rather than extend it.** It is
redundant with `LinkFeatures` and strictly less general.

**[DONE — phase-351 W6, 2026-08-15]** Checked, and nothing read it: `net_stack`
was parsed and never consulted from the day it was added, for either meaning.
It is deleted, along with the `NetStack` type. The question it half-answered
("who owns NIC + IP bring-up") is a property of the integration shell and is
carried by RFC-0072's A/B split; the question consumers actually ask — *which*
stack this board can be built with — is `supported_netstacks` (phase-351 W4),
which IS read: by `BoardDescriptor::resolve_netstack`, by `nros dev image board-facts`,
and by `check-site-config`.

## Where board support survives, and why

Three consumption surfaces, decided by one question: **does the host ecosystem
have a build system?**

| Surface | When | Cost | Examples |
|---|---|---|---|
| **Integration shell** | ecosystem owns boards + build | ≤200 LoC once **per ecosystem**, then zero per board | Zephyr, ESP-IDF, NuttX, PX4, PlatformIO, vendor SDKs |
| **Board module** | no ecosystem build system — nano-ros must produce the image | per **board** | bare FreeRTOS, bare ThreadX, bare metal |
| **Nothing** | hosted | zero | POSIX, native |

Only the middle row is genuine "board support", and it exists only because bare
FreeRTOS and bare ThreadX ship a kernel and nothing else — no board abstraction,
no driver model, no linker script, no build. Someone must supply those, and if
the vendor has not, it falls to whoever is porting.

Two consequences worth stating plainly:

- **The middle row should shrink, not be systematised.** A vendor SDK build
  (S32DS, MCUXpresso, STM32CubeIDE) is an ecosystem with a build system, so it
  belongs in row 1 — a shell, not a board module. The MR-CANHUBK3 FreeRTOS
  deliverable is an S32DS project with `.cproject`, `Debug_FLASH/makefile`, RTD
  drivers, startup and two linker scripts. **The right integration may be
  `integrations/s32ds/` rather than `cmake/board/nano-ros-board-canhubk344-freertos.cmake`.**
  That inverts the plan RFC-0064 R1 proposed and is the most important open
  question in this document. **[OPEN — test it during the CANHUBK3 bring-up; issue 2539.]**
- **For the boards that genuinely stay in row 2, keep the module dumb.** It
  supplies linker script, startup, kernel config, netif, and nothing else. It
  should not carry arch policy, language standard, or config that belongs to the
  platform or the app.

## What landed, and what did not (2026-11-14)

The full sequencing is in the companion. Checked against the tree on
2026-11-14:

- **Landed:** step 0 and 1 (Cortex-M7 FreeRTOS unblock, board-less FreeRTOS),
  step 2 (`integrations/s32ds/`), the `net_stack` question (nothing read it),
  the native → `linux` rename (phase-500), the NuttX QEMU board merge (phase-337 W3), the
  ThreadX riscv64 arch port (`nros-board-threadx-port-riscv64`), the Zephyr
  Cortex-M witness (`mps2/an385`, phase-337 W2.a), and Revision 5 (phase-375
  W6 to W9, phase-215 215.K, phase-420 W3).
- **Did not land:**
  - step 6, re-keying `[board.*]` in the SDK index as `[profile.*]`:
    [issue 2537](../issues/2537-board-index-tables-not-profiles.md);
  - step 8, extending the RFC-0049 ladder to the build block (a board
    contributing `[arch.*]`, `include_paths`, `required_env`):
    [issue 2538](../issues/2538-board-layer-build-block-not-on-ladder.md).

## Revision 5 (2026-09-06) — one board process, and the descriptor holds only primitives

Written after auditing every board in the tree to answer a simple question: does
a board of ours arrive the same way a user's board does? It does not. Revision 2
established that nano-ros declares what it needs and the integrator satisfies it;
revision 5 makes the *arrival* uniform, so that "no builtin road" is a property of
the code rather than a statement in a document.

R5 changes no R2/R3/R4 principle. It applies them to the descriptor and to
discovery, and it deletes the one board that grew its own format.

### The measurement — four processes, not one

Twelve `[[board]]` rows across eleven `nros-board.toml` files, plus one board
with no descriptor at all (2026-09-06):

| Route | How a board is added | Boards |
| --- | --- | --- |
| 1. Package | crate + `nros-board.toml` + `package.xml` announcing `<nano_ros_provides kind="board">` | 9 |
| 2. Descriptor-only package | route 1 minus the crate (`packages/boards/{linux,zephyr}/`) | 2 |
| 3. Bundle + `board.cmake` | 14 `NROS_BOARD_*` cmake variables, a Cargo mirror, and a `west` extension | 1 (`fvp-aemv8r-smp`) |
| 4. Nothing at all | an inline `board = "…"` string in `examples/fixtures.toml` plus a `.conf` hand-placed in each example leaf | the rest |

Route 4 is the finding that reframes this. Three Zephyr board ids are actually
built — `native_sim/native/64`, `mps2_an385`, `qemu_cortex_a53/qemu_cortex_a53/smp`
— and only the first is declared anywhere, wedged into the `zephyr` descriptor's
`names` array beside a real name. `qemu_cortex_a53` has no package, no descriptor
and no announcement; its per-board `.conf` lives in the example leaf that uses
it, so a second consumer copies the file. **Our own QEMU board did the same job
as the FVP board with zero files, while the FVP board spent eight plus a west
verb.** Neither is the process.

Three mechanisms are correct and unreachable — the class this tree keeps
re-finding (issues 0896, 0963, and `check-test-scripts-have-callers`):

1. **The phase-215 drift gate checks zero boards.**
   `phase215_f_manifest_drift.rs` walks top-level `packages/boards/nros-board-*/`
   dirs needing both `Cargo.toml` and `board.cmake`. The only `board.cmake` in
   the tree sits one level deeper, at
   `nros-board-zephyr/boards/fvp-aemv8r-smp/`, so the loop never reaches it —
   and no board carries `[package.metadata.nros.board]` at all, so even a fixed
   glob would hit `if msg.contains("no `[package.metadata.nros.board]`")
   { continue; }`. The gate guards an invariant nothing satisfies.

2. **The announcement is optional, so the newest board skipped it.**
   `check-provider-announcements.py` reads `if not os.path.exists(pkg_xml):
   continue  # not migrated yet`. `nros-board-mps3-an536-freertos` (phase-385,
   the most recent board) has a descriptor and no `package.xml`. Nothing said so.

3. **Discovery is one level deep, and the bundle case is patched with a
   `board.cmake` read.** `BoardCatalog::load_root` scans
   `<root>/*/nros-board.toml` and no deeper, so a bundle board cannot carry a
   descriptor of its own. `fvp-aemv8r-smp` resolves today only because
   `attach_bundle_aliases` walks `nros-board-<family>/boards/*/board.cmake` and
   appends the bundle name and its `NROS_BOARD_ZEPHYR_ID` as ALIASES onto the
   family descriptor — so the FVP board has no descriptor, it borrows the
   `zephyr` one. D4 deletes `board.cmake`, which deletes that alias path, so the
   depth fix is a prerequisite rather than a tidy-up.

   **The out-of-tree ROOTS, by contrast, are already right, and an earlier draft
   of this revision said otherwise.** `extra_board_roots()` reads a PATH-style
   `NROS_EXTRA_BOARD_PATH`, and `load_with_packages` absorbs a board declared
   inside the user's own workspace package — the colcon property that a
   workspace carries what it needs. What D2 asks for is the DEPTH and one scan
   shared with `provider_scan`, not the root list, which exists.

   Note none of this is what phase-346 fixed: that phase made the board
   FRAMEWORK resolvable from an out-of-tree leaf, through the Entry's build
   script. Descriptor discovery was never part of it.

### D1 — A board is a package, and that is the whole identity

Adopt RFC-0087 D1 for boards without exception: a directory with a `package.xml`
announcing `<nano_ros_provides kind="board" name="…"/>`, plus a sibling
`nros-board.toml` saying what it lowers to. A crate only if the board needs
bring-up code.

```
<any dir>/
├── package.xml        <nano_ros_provides kind="board" name="…"/>   ← identity
├── nros-board.toml    what it lowers to                            ← behaviour
├── prj.conf           optional, Zephyr only
├── boards/<flat>.conf / .overlay   optional, derived by convention
└── Cargo.toml, src/   optional — rung 2 of the customization ladder
```

Ours and a user's differ in exactly one respect: which search root found them.

### D2 — One scan, depth-independent, shared with `provider_scan`

The root list is already right (see the measurement's point 3). Two things are
not:

* **Depth.** `load_root` stops at one level, so a bundle board cannot own a
  descriptor and is patched in by reading `board.cmake` — the file D4 deletes.
* **Two scans.** `provider_scan` already walks `package.xml` with
  `COLCON_IGNORE` / `AMENT_IGNORE` / `NROS_IGNORE` honoured;
  `BoardCatalog::load_root` walks descriptors separately with its own rules.
  Boards are found by the descriptor walk and announced to the other one, which
  is exactly how a board comes to have a descriptor and no announcement.

D2 is therefore: boards are discovered by `provider_scan` like every other
provider, and `nros-board.toml` is read for a package that announced itself as a
board. That collapses the two walks, makes depth a non-question, and makes the
missing `package.xml` a resolution failure instead of a silent skip.

### D3 — `[board.zephyr]`, so routes 3 and 4 stop existing

One block carries the Zephyr facts a board needs, and conventions supply the
rest:

* `west_board` — the Zephyr board id. It stops being a string in
  `fixtures.toml` (route 4) and a `NROS_BOARD_ZEPHYR_ID` cmake variable
  (route 3), and it stops being smuggled through `names`.
* ``boards/<west_board with `/` → `_`>.{conf,overlay}`` — derived,
  present-if-exists. `board.cmake` already stated this rule in a comment and
  then hand-wrote both paths anyway.
* `prj.conf` — derived, present-if-exists.
* `runner` — from Zephyr's own board definition; stated only when it differs
  (the FVP's `armfvp` does).
* a Rust-support Kconfig module — derived and GENERATED, since its whole body is
  `default y if BOARD_<UPPER(first segment of west_board)>`.

### D4 — cmake reads a generated projection, never a second authored file

`nano_ros_use_board(<name>)` keeps its signature — a downstream consumer's call
site does not move — but its body runs `nros dev board cmake-vars <name> --out
<build-dir>/…` and `include()`s the result. A build artifact, never committed
(the SystemModel precedent, phase-330). `board.cmake` and the
`[package.metadata.nros.board]` mirror are deleted, and with them the two-faces
drift problem rather than the vacuous gate that watched for it.

The replacement test is a round-trip — descriptor → projection → parse back ==
descriptor — which cannot go vacuous, because it has no "skip if the other face
is missing" arm.

### D5 — The announcement becomes mandatory

`check-provider-announcements` loses the `continue`: a descriptor with no
sibling `package.xml` is an error, ratcheted against today's single offender.

### D6 — The descriptor holds primitives only

A field audit over all 12 rows. The rule is RFC-0087 D4's — a descriptor carries
only what no convention can produce — plus RFC-0049's duty rule, which says a
software-stack fact belongs to the platform and a hardware fact to the board.

**Keep (primitive):** `names`, `platform`, `supported_netstacks`,
`[board.capabilities]` heap/atomics/threads, `board_crate`, `target`, `chip`,
`[board.cmake] toolchain_file`.

**Move to the platform descriptor** — the board is copying a fact it can get
wrong:

| Field | Evidence |
| --- | --- |
| `platform_feature` (12/12) | 3 of 12 are NOT `platform-<platform>`: esp32 → `platform-bare-metal`, threadx-linux and threadx-riscv64 → `platform-threadx` |
| `link_kind` (12/12) | 11 `none`, 1 `nuttx-staging` — and BOTH NuttX rows carry it |
| `[board.priority_plan]` (6) | `mps2-an385-freertos` and `mps3-an536-freertos` hold byte-identical blocks (`[1,7]`, transport `[4,4]`, app `[1,3]`). That is `configMAX_PRIORITIES`, a FreeRTOS fact. A board that retunes it still overrides |

The Zephyr row's `resolver = "scripts/lib/priority_plan.py:resolve_zephyr_plan"`
is a file path and a Python symbol living in a board descriptor. It moves with
the block.

**The move has a prerequisite this audit did not see.** Only `config/bare-metal`
and `config/generic` HAVE an `nros-platform.toml`; the other five platforms
carry a `package.xml` and nothing else. "Move it to the platform descriptor"
assumes a descriptor to move it into, and for five of seven platforms there is
none. Creating them is its own wave, and it is where the move belongs.

**Derive, override permitted:**

| Field | Rule | Exceptions today |
| --- | --- | --- |
| ~~`entry_kind` (12)~~ | **WRONG — corrected 2026-09-06.** The rule was stated as "hosted platform → `hosted-main`; zephyr → `zephyr-staticlib`; else `board-run`, no exceptions". Measured while implementing: `freertos-posix` and `mps2-an385-freertos` are BOTH `platform = "freertos"` and their values are `hosted-main` and `board-run`. Whether a port is hosted is a fact no field carries, so the derivation needs a NEW field — not a simplification. It stays authored. | — |
| `local_aliases` (10) | default `[platform_feature]` | 2 real (esp32, threadx-riscv64) |
| `[board.entry] crate_name` (7) | `snake_case(board_crate)` | none — 7 of 7 |
| `[board.entry] signature` (7) | default `#[unsafe(no_mangle)] extern "C" fn main() -> !` | 3 real (esp-hal, cortex-m-rt, plain `fn main`) |

**Decompose.** `cargo_config` (8/12) is a raw TOML blob — a TOML document inside
a TOML document, with no schema check on the inner one. The `rustflags` (8),
`runner` (3) and `linker` (2) counts in this audit came from *inside* it as
text; those primitives do not exist as fields. They should:

```toml
# today
cargo_config = '''
[target.thumbv7m-none-eabi]
runner = "qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic …"
rustflags = ["-C","link-arg=-Tlink.x","-C","link-arg=--gc-sections"]
'''

# after
target    = "thumbv7m-none-eabi"
runner    = "qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic …"
rustflags = ["-C","link-arg=-Tlink.x","-C","link-arg=--gc-sections"]
```

The CLI already composes the leaf `.cargo/config.toml`; it should build it from
those rather than paste a blob through. This is also the field a user is most
likely to get wrong, and the one they get the least help with.

**Drop:**

* `capability_features` (7) — all seven hold the identical value
  `["safety-e2e"]`, and the only read in the entire tree is
  `board_descriptor.rs:1118`, inside
  `fn a_board_advertises_the_safety_capability_feature()`. A field whose sole
  reader is a test asserting that the field is used.
* `[board.entry] comment` (4) — a Rust `//` comment stored as an escaped
  multi-line TOML string. Presentation, not a board fact.
* ~~`crate_path`, `board_features`~~ — **WRONG — corrected 2026-09-06.** Authored
  by no in-tree descriptor, which is what the audit measured, but
  `builder/entry.rs` reads both. They are an out-of-tree extension point with no
  in-tree user, not fields to delete.

**Keep but reclassify:** `target_contains` (2) is not a board property; it is a
disambiguator so two `[[board]]` rows in one file can be told apart by target
substring. Rename it to say so.

**Left alone, flagged:** `toolchain` (10 stable / 2 nightly). Both nightly rows
sit on custom targets needing `build-std`, which suggests a target fact rather
than a board one. Not enough evidence to move it.

Rung 3 of the customization ladder survives intact: `crate_root_extra`,
`crate_root_deps` and `closure_extra` remain the escape hatch, renamed to say
that is what they are. A model that cannot be escaped is worse than none.

### What it costs to add a board, after

A QEMU Zephyr board authors **6 keys**. Today's equivalent authors 19.

```toml
[[board]]
names               = ["qemu-cortex-a53"]
platform            = "zephyr"
supported_netstacks = []

[board.zephyr]
west_board = "qemu_cortex_a53/qemu_cortex_a53/smp"

[board.capabilities]
heap = true
atomics = true
threads = true
```

`platform_feature`, `local_aliases`, `link_kind`, `entry_kind`, `toolchain` and
the priority plan all follow from `platform = "zephyr"`.

A real out-of-tree board on real silicon — the MR-CANHUBK3 this RFC was written
against — authors the same shape plus what genuinely differs: a crate for rung-2
bring-up (`nros_board_init_eth()`), a `[board.cmake] toolchain_file`, a linker
script in `rustflags`, and a `[board.provisioning] gated` naming its own SDK
entry. Field for field, that is the in-tree `s32z270-freertos` descriptor. The
only difference is which root found it — which is the whole claim of D1 and D2.
Today the ROOT part of that already works (`NROS_EXTRA_BOARD_PATH`, and a board
declared inside the consumer's own workspace package); what does not is that a
board is found by a different walk from every other provider, and so can exist
without ever announcing itself.

### Gates this revision asks for

* `check-board-descriptor-single-source` — no `board.cmake` anywhere, no
  `[package.metadata.nros.board]` anywhere.
* The projection round-trip, replacing `phase215_f_manifest_drift.rs`.
* D4's ratchet, in RFC-0087's shape: a stated derivable field must equal its
  derived value, and a moved field must not be restated at board level unless it
  differs.
* `check-provider-announcements` without the `continue` (D5).

Tracked by [phase-375](../roadmap/archived/phase-375-board-tier-policy-and-onboarding-cost.md)
W6–W9; the FVP migration is [phase-215](../roadmap/archived/phase-215-board-crate-as-importable-unit.md)
215.K. The package-shape half lands inside
[phase-420](../roadmap/archived/phase-420-package-identity-and-provider-format.md) W3.

### Amendment (2026-10-10) — every framework id is a typed field, never a `names` entry

Issue 1519 moved Zephyr's board string out of `names` into `[board.zephyr]
west_board`. Issue 1652 finishes the rule for the two ecosystems that had no
typed home: NuttX's board-plus-config (`[board.nuttx] board_config =
"qemu-armv7a-nsh"`) and PlatformIO's board id (`[board.platformio] board =
"esp32dev"`). The rule is the same for all three: a framework id RESOLVES
(`BoardDescriptor::answers_to`, so a `[deploy.*].board` may carry it, issue
0606) and is REFUSED on an `[image.*] board` (`framework_board` /
`refuse_framework_board`, mirrored by `check-deploy-board-resolves`). `names`
holds nano-ros's names only, and `package.xml`'s board announcements follow it.
`west build -b` reads `zephyr_board()` alone, so a non-Zephyr id can never
reach it.

## Open questions

Re-checked 2026-11-14. Four are still open and are filed together as
[issue 2539](../issues/2539-board-support-organization-open-questions.md):

- **[OPEN]** Is a vendor SDK build (S32DS/RTD) row 1 or row 2? Step 2 landed
  `integrations/s32ds/`, but the answer was never written here.
- **[OPEN]** What is the mandatory subset of the platform ABI?
- **[OPEN]** `[board.fvp-aemv8r-smp] platform = "bare-metal"` in the index vs
  the board being a Zephyr board — drift, or does the index's `platform` mean
  "provisioning family"?
- **[OPEN]** Should there be an `integrations/generic-cmake/` shell?

Answered:

- Does anything read `net_stack` for build responsibility? **No** (R3,
  2026-08-04).
- `config/bare-metal`'s unreachable `[arch.cortex-m4f]`: **resolved** (issue
  1154, 2026-10-24).
- The three board modules selected by no caller (`esp32-c3`, `mps2-an385`,
  `stm32f4-nucleo`): `esp32-c3` was retired (RFC-0113 D5), the `stm32f4`
  board went with phase-337 W7.a, and `mps2-an385` is the live bare-metal
  board. None of the three is a question any more.
- Which QEMU-able Zephyr board for the Cortex-M witness: **`mps2/an385` via
  `smsc911x`** (phase-337 W2.a).
- Does a hosted non-Linux target earn a row? **No claim is made**: the host
  board is `linux` only (phase-500; it fails on darwin, windows and freebsd).

## Review (2026-11-13, phase-555 W2)

**Verdict: amend.** The design question is brief item P7. Nothing in the
body was edited.

- All three tracked phases are archived:
  - phase-337 (all waves landed);
  - phase-375 (archived 2026-10-11 with W0 to W9 landed; its own status
    line was left saying "W2 to W9 open");
  - phase-215 (complete).
  The document still calls itself "LIVE — under active exploration".
- 118 asked whether Sequencing step 8 landed: **no.** Step 8 is "extend the
  RFC-0049 ladder to the build block: the board layer contributes
  `[arch.*]`, `include_paths`, `required_env`".
  - `nros-platform-config/src/platform_config.rs` reads `arch_profiles`,
    `compile`, `required_env` and `[arch.*]` from the PLATFORM file only.
  - Board descriptors carry `[capabilities]` and `[knobs]` deltas, as the
    module doc says, and no build block.
  - No `nros-board.toml` has an `[arch.*]`, `include_paths` or
    `required_env` key.
  - The `-mfpu` the step was meant to pay off is now in toolchain files and
    `NanoRosCargoProfile.cmake`, not in board descriptors.
- Other steps checked:
  - Step 2 landed: `integrations/s32ds/`.
  - Step 6 did not (`[board.*]` to `[profile.*]`): `nros-sdk-index.toml`
    has no `[profile.*]` table.
  - Several `[OPEN]` questions are still open (the vendor-SDK row, the
    mandatory ABI subset, the FVP `platform` field, `generic-cmake`).
- Already superseded in part, and recorded at the top: the book rows (by
  RFC-0114, RFC-0115 and RFC-0124), and "The supported matrix" tier
  definitions (by RFC-0114 D10/D11, which `amends: [RFC-0064]`).
- Stale references (spot-checked): the `board_descriptor.rs` line numbers in
  the open questions predate the move to `nros-cli-core/src/orchestration/`.
  The prose is history and was not rewritten.
- Pairs:
  - RFC-0012: 0064 says it finishes it. No contradiction; RFC-0012 is 118's
    to review.
  - RFC-0040, RFC-0049, RFC-0072, RFC-0101, RFC-0114, RFC-0115: each
    extends or narrows 0064 by an explicit amendment. None contradicts R2's
    stance.
- `area:` platform-board-rtos is right.

**W3 (2026-11-14).** The user ruled P7 (b). Applied: split into this Stable
RFC (stance, the seam, Revision 5, open questions) and the archived
[exploration record](archived/board-support-organization-exploration-log.md),
linked both ways; steps 6 and 8 filed as issues 2537 and 2538 and the four
open questions as issue 2539; the banner names RFC-0114, RFC-0115 and RFC-0124
as superseding the tiers and book rows (each carries `amends: [RFC-0064]`), and
`amends` names RFC-0012. ARCHITECTURE §4 updated.
