# nano-ros Issues

This directory tracks nano-ros bugs, limitations, and tech-debt — one file
per issue, mirroring the repo's numbered-RFC convention
(`docs/design/NNNN-*.md`) and the roadmap `archived/` pattern. Each file
carries YAML frontmatter plus the issue body (problem, evidence, current
state, fix/direction). Open issues live directly in `docs/issues/`; resolved
ones move to `docs/issues/archived/`.

## Conventions

**Frontmatter schema** (every issue file):

```yaml
---
id: 7                    # the issue number (matches the 4-digit filename id)
title: Unbounded message sequences capped at 64 elements
status: open             # open | resolved | wontfix
type: enhancement        # bug | enhancement | tech-debt
area: codegen            # codegen | rmw | memory | cmake | zephyr | nuttx | freertos | threadx | build | testing
related: []              # e.g. [rfc-0023, phase-218] — cross-links to RFCs / phases
resolved_in:             # (resolved only) commit short-hash or phase, e.g. "Phase 140"
---
```

**Lifecycle**:

1. Open an issue as `docs/issues/NNNN-slug.md` with `status: open`.
2. When resolved, set `status: resolved` + `resolved_in:` and **move** the
   file to `docs/issues/archived/NNNN-slug.md` (trimmed to a terse
   resolution summary).
3. **Numbering** = the next integer after the highest existing id.
   **Slug** = a kebab-case form of the title; the filename id is the
   zero-padded 4-digit issue number.

## Issue vs RFC vs phase doc

- **Issue** (`docs/issues/`) = a bug, limitation, or tech-debt item.
- **RFC** (`docs/design/NNNN-*.md`) = a design decision.
- **Roadmap phase** (`docs/roadmap/`) = an implementation plan.

Issues cross-link to the RFCs and phases that inform or resolve them via the
`related:` frontmatter field.

## Open issues

- **#198** — [ESP-IDF registry publish never executed, no CI](0198-esp-idf-registry-publish-unexecuted.md):
  the one distribution surface left open by #171/phase-288 — and a compote dry-run pack shows the
  component is structurally unpublishable as laid out (3-file shell, `_nros_root=../..` escapes
  the archive). Needs a layout decision (root manifest packing the tree vs wontfix) BEFORE the
  maintainer credentials + CI lane; the doc's deprecated `idf.py upload-component` is fixed.
- **#195** — [threadx-riscv64 cyclone two-qemu pubsub: boots, 0 delivery](0195-threadx-riscv64-cyclone-two-qemu-zero-delivery.md):
  deterministic on fresh fixtures; delivery assert (not readiness) — check pair identity/domain
  first (0161 class), then the riscv64 rebuild pitfalls (0131/0138/sizes-header race).
- **#190** — [esp32 QEMU e2e: boots, 0 delivery](0190-esp32-qemu-e2e-zero-delivery.md):
  lane restored to the sweep (#181), boots + logging green; the four cross-delivery tests get 0
  samples — check pair identity/port drift first (the #179/#181 lessons), then the #64 heap notes.
- **#191** — [freertos rust entries: boot + connect, 0 delivery](0191-freertos-rust-entry-zero-delivery.md):
  lane repaired end-to-end by #181 (entry images, per-variant ports, IP split, launcher/gates);
  session opens, nothing publishes — entry runtime path, never previously exercised.
- **#183** — [declarative ws-bridge lanes deliver 0 samples](0183-declarative-bridge-lanes-zero-samples.md):
  zenoh→cyclonedds (nano listener + nested-header) and zenoh→xrce; bridged-side listener prints
  NOTHING → entry likely never comes up. Imperative bridge + demo_nodes interop pass serialized.
- **#178** — [RTIC images never deliver — `Executor::open` blocks in `#[init]`](0178-rtic-executor-open-blocks-in-init.md):
  every `deploy = "rtic-*"` qemu-arm-baremetal image boots + brings up the network but hangs at
  `Executor::open` (the blocking zenoh connect) inside RTIC `#[init]`, where interrupts are masked →
  smoltcp gets no timer/RX IRQ → the TCP handshake never completes → `published=0`. All four
  `test_qemu_rtic_*_e2e` fail with zero delivery. Fix (architectural) = move the session open out of
  `#[init]` into the `__nros_run` task (after interrupts unmask). Runtime-only — `just check` green.
- **#165** — [riscv-nuttx board has no `run_tiers` (RFC-0015 Model-1)
  seam](0165-riscv-nuttx-run-tiers-model1-seam-absent.md): `QemuRvVirt` wires only the
  single-tier Entry path; the arm sibling's `impl { run_tiers }` (+ `entry_net_init` eth0 push)
  has no riscv twin. The board + one C `talker` are **link-checked** in nightly CI
  (`build-riscv-c`), but there is **no rv-virt NuttX boot harness** (`start_nuttx_virt` is
  arm-only) — riscv-nuttx fixtures never run, so the seam is e2e-unprovable. Not a matrix axis
  (nuttx cells are arm-only by design). Tracked, not silent; blocked on a runtime boot harness.

Recently resolved (see [`archived/`](archived/) for the full list): **#171** — the
no-external-distribution umbrella closes: D1/D2 source-distribution bootstrap (phase-288), the
RFC-0048 ament CMake shape + W9 Rust consumption (phase-287, complete), false claims all
truth-fixed; the single live remainder (ESP-IDF registry execution) is #198. **#194** — the
threadx-linux rust rtos-e2e zero-delivery was three stacked defects, none in the runtime:
museum pre-212.L role binaries satisfied a retired builder path (the freertos #181 entry-image
repair was never applied here), the board crate lacked the #131 `rmw-zenoh` forwarding so every
entry image booted with NoBackend (`Executor::open` ConnectionFailed, zero wire I/O), and the
rust entry `main` lost `startup.c`'s `setvbuf` so a piped harness never saw the readiness
banner. Entry builders + markers + per-variant baked ports + board feature + stdout
line-buffering landed; pubsub/service/action all pass. Re-triage #191 against the same causes.
**#192** — the FVP
`getentropy` link red was the #193 CMake<3.24 whole-archive flag-dedup class on the ZEPHYR
generator: three-item `-Wl,--whole-archive <ffi.a> -Wl,--no-whole-archive` triples collapsed into
an UNCLOSED bracket that swallowed picolibc's `-lc` whole-archive → every `libc_ssp_*` member
force-included → `__stack_chk_init` → undefined `getentropy` (nothing in-tree references
`__stack_chk_*` at all). Fix: one comma-joined token per lib; FVP lane links + smoke OK,
cpp-talker-zenoh regression green. **#196** — not a probe hole: `examples/fixtures.toml` simply
had NO `rmw = "zenoh"` variant row for rust/service-client-callback (every sibling has one), so
no sweep ever built the `target-zenoh/` binary the test consumes. Row added; both rust-client
interop tests pass on a sweep-built binary. Full consumer↔manifest audit: no other native gap
(px4 pair intentionally owned by `just px4 build-fixtures`). **#189** — both baremetal
serial lanes revived. Zenoh-serial: the provisioned zenohd lost `transport_serial` in the
phase-187 migration (router exited on the serial listener) AND serial-only firmware compiled the
frozen-clock smoltcp spin branch (`ZPICO_SMOLTCP` hardcoded in the Phase-136.4 manifest) — zenohd
reprovisioned `1.7.2-nros2` + provenance-aware setup, runner swaps in `ZPICO_SERIAL`. XRCE: the
image registered NO RMW backend at all (#163 class — `__register_linked_rmw()` is a Phase-249
no-op, the board's explicit register covers only `rmw-zenoh`, linkme is dead on bare-metal), so
`Executor::open` failed before one byte hit the UART; `setup_transport` now calls
`nros_rmw_xrce_cffi::register()`, and the documented `serial/...` → custom-vtable locator route
is actually implemented on non-POSIX. All lanes green (xrce+serial+ethernet emulator, POSIX XRCE
10/10). **#197** — the pure-C
workspace (`examples/workspaces/c`) aborted cmake-configure with `missing-source-metadata` for
c_talker_pkg/c_listener_pkg. Root cause: a STALE in-tree `nros` (built 2 days before 287-W6's
`nano_ros_add_node` ament verb + its `parse_add_node_call` parser landed), so it parsed zero
components from the migrated CMakeLists. Fixed by rebuilding the CLI; added a fail-loud
CLI-staleness guard to `nros_require_ws_sync` so a stale CLI can't silently break workspace
planning at configure again. **#188** — the nuttx
C/C++ action + C++ service reds were the #153 gossip gap unported: ret=-2 is TIMEOUT (not a
rejection) on a query fired before the server's queryable gossips (a zenoh get only matches
queryables visible at fire time). The native rust demos got the 3-attempt/1 s-backoff retry in
#153; ported to the three nuttx clients (fresh query per attempt; retries only on -2). All six
nuttx action+service lanes 6/6, fixed lanes 3× serialized. freertos/threadx copies carry the
same latent window (noted in the archive). **#187** — the W7
class-prefix lint compared verbatim hyphenated Cargo names against Rust paths (unsatisfiable;
22 leaves red by resolution). The consumer (`resolved_crate_name`) canonically maps pkg → crate
ident (`-`→`_`), and the older sibling lint already normalized; the W7 walker now compares the
crate-ident prefix. Seeded-violation verified. **#193** — fresh native
cyclone C listener `register_subscription -> -1` was `find_descriptor -> nullptr`: on CMake < 3.24
the descriptor ts lib's static-init ctors were GC'd because the `-Wl,--whole-archive <target-name>`
group let CMake de-dupe the archive out. Fixed with the de-dup-safe pre-3.24 idiom —
`target_link_options(... "SHELL:-Wl,--whole-archive $<TARGET_FILE:…> -Wl,--no-whole-archive")` — no
3.24 requirement (kept the #181 3.22 floor). Verified on CMake 3.22.1: 30 ctors link, register
succeeds, and the #183 bridge chain delivers e2e. **#186** — test rot
deleted, not repaired (maintainer call): the three integration shell
smokes probed layouts retired in 208.D.7/D.8/D.10 and could never run again (canonical shapes
covered by `cli_bringup_*` + the west fixtures), and the whole hidden `nros migrate workspace`
verb went with them — its "release pin" drift gate was a tautology (post-218 there is no pin,
and the in-tree emitter never adopted the post-212.I sub-table). Pre-212 trees migrate via the
nros-v0.5.0 tag's CLI (breaking-removal note in the archive + diagnostics). **#185** — the "half-baked
shim" was a museum WEST fixture, not an emitter bug: no current code path can emit
`system_config.h` without `.cmake` (single writer + the shim FATALs on either missing), and all
four lanes pass 4/4 on fresh fixtures — the three suspected phase-287 commits are innocent. The
west `.compile-ok` stamp was date-only (no tool identity), so the sweep consumed a partial
sweep-era bake; it now stamps the `nros` CLI's sha256 and `require_west_fixture` fails loud on
mismatch (negative-tested). The fifth bullet (zephyr workspace-entry e2e) stays with the
#164/#181 zephyr-family rebuild. **#181** — the fixture sweep
no longer exits 0 with unbuilt lanes: esp32+px4 lanes added to both drivers, esp32 ELF-name drift
+ in-test builds removed, freertos rust rewired to the *-entry images with per-variant ports;
residuals split to #191/#190. **#184** — the baremetal
serial/XRCE OOM wasn't a missed board default: the three images PIN `NROS_HEAP_SIZE=24576`
(phase-204.5 size recipe) in their `.cargo/config.toml`, unbootable once the phase-271 executor
backing became a single ~75 KB allocation. Pins → 131072 (the #176 default; `.bss`, no flash
cost) + the book's size-minimal recipe corrected (its 24 KB advice OOMs every `nros::main!`
image; the published pre-271 footprint RAM rows are stale pending re-measure). The
`max_callbacks` shrink route was rejected: arena floor + XRCE session still bust 24 KB, and the
`_sized` seam is posix-only. Images boot past allocation; the deeper session-open failure split
to #189. **#179** — zenoh action
get-result deserialize (ALL platforms): offset-5 slices + unconditional trampoline re-header, three
bugs cross-validating; one delivered-with-single-encap contract everywhere — native matrix 5/5,
ws roundtrips 4/4, freertos+threadx-linux e2e 4/4. **#182** — the realtime-tier
"no differentiation" (nuttx c/cpp tiers + cpp subnode, ctrl==telem) was NOT a scheduling bug: all
five lanes pass on truly-fresh fixtures. The sweep's fixtures ran museum GENERATED entry TUs —
both the configure-time entry codegen (`CMAKE_CONFIGURE_DEPENDS`) and the workspace-fixture input
signature were blind to the `nros` CLI binary, so the Jul-8 group-split/tier emitter fixes never
re-ran. Two guards landed: the CLI joins `CMAKE_CONFIGURE_DEPENDS` (rebuild → codegen re-runs;
byte-identical output skips the rewrite) and the signature (v2) hashes the CLI content, so a
stale-tool fixture now fails loud at test time. Pre-v2 stamps read stale until each family
rebuilds once. **#177** — native/threadx-linux
cyclone duplicate `register_<Type>_0` link failure: idlc register ctors now package-namespaced
(`register_<pkg>_<stem>_<idx>`, `fd7d42b87`); both cyclone fixture lanes link green. **#164** — the tests/zephyr.rs
"mass rot" (29/45 fail on fresh images) is fully drained: every lane resolved to a stale marker
(fixed), the #163 backend gap, the #147 staleness false-positive (phase-286 W2), a spun-off delivery
bug (#173/#174/#175/#180), or the mtime treadmill — no RMW defect left; the formerly-`#[ignore]`d
zenoh C action test was a stale-marker false "hang" and now passes. **#180** — the zephyr-service →
native-client "no reply" was the #153 gossip-gap (server liveliness gossips ahead of its queryable
route); the native service client's retry was widened `3×1s → 8×2s` to span the slow-pico window
(native path unchanged). **#173** — Zephyr pub → native
sub "no delivery" was a **stale-fixture false alarm**: the prebuilt native listener was Int32-era
while its source migrated to `std_msgs/String`, and the #164 cross tests ran it under
`NROS_SKIP_FIXTURE_CHECK=1` (bypassing the staleness guard) → keyexpr type mismatch (`Int32_` vs
`String_`) → 0 delivery. Rebuild the fixture → all four cross lanes (rust+cpp, both directions) PASS.
No RMW code change. **#175** — Zephyr Cyclone
action completion (all three lanes): rust nested-message encap-splice + typed dispatch
(`844021843`/`e9bb39686`) and the C/C++ server register `-100` (ROS-slash vs DDS-mangled feedback
type in `find_descriptor`, fixed via `ros_form_to_dds` normalisation; `facd36ca4`) — `dds_{c,cpp,rs}_action_e2e`
all PASS (phase-286 W4). **#166** — Zephyr zenoh e2e baked-port serialization: native_sim
`-testargs --nros-locator` runtime override → per-test ephemeral zenohd; all six
`qemu-zephyr-{pubsub,service,action}-{rust,cpp}` groups + the ws-entry lane now parallel (phase-286
W1). **#176** — RTIC mps2-an385
heap OOM (`memory allocation of 74888 bytes failed`): the per-entry executor backing is a single
~74888 B alloc that overflowed the 64 KB non-tls default heap. Fixed by raising the mps2-an385
default heap to 128 KB (`ae0aecaa6`; MPS2 has 16 MB RAM, `HEAP` is `.bss`). The RTIC e2e still fail
downstream on the separate init-time connect hang (#178). **#167** — riscv-nuttx
boot panic (`EPC=0x4`) was a `struct pollfd` ABI mismatch: NuttX's kernel `pollfd` is 24 bytes
and its flat-build `poll()` writes all six fields into the caller's array, but Rust std/libc use
the 8-byte POSIX `pollfd`, so std's `sanitize_standard_fds()` (fds 0/1/2 on the entry task's
stack) overflowed by 48 bytes and smashed the saved return address. Fixed with a `-Wl,--wrap=poll`
shim (`jerry73204/libc` `nuttx-0.2` @ `adb4c592e` + superproject `d06d25fa4`); boot-verified. The
"timing-dependent virtio-net race" reading was a red herring of arm-vs-riscv stack-layout
sensitivity. **#174** — Zephyr XRCE
C/C++ "0-delivery" was a missing agent locator (the C/C++ analog of #163): `NROS_ENTRY_LOCATOR`
(`nros-cpp/main.hpp`) only read `CONFIG_NROS_ZENOH_LOCATOR` (unset for XRCE) → `""` → the XRCE
transport never connected (`run_components` rc=-100). `main.hpp` now synthesizes the agent
`host:port` from `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}`; plus 3 stale action markers fixed. All 6
XRCE C/C++ lanes green (phase-286 W3). **#80** (wontfix) —
on-device parameter persistence is a non-goal; params are authored in launch files and the
build system bakes them as node defaults (`orchestration/params.rs` → `declare_param` codegen).
The dormant `ParamStore`/`FileParamStore`/persist seam is now dead code (harmless `NullParamStore`
no-op default), flagged for optional cleanup in the archived issue. **#170** — every
canonical example leaf (176) now ships a copy-out README, generated from facts read off the
leaf by `scripts/docs/gen-example-readmes.py` (hand-written pages preserved; absolute GitHub
links since a copied-out dir has no repo above it), gated by
`example_shape::every_canonical_leaf_has_readme`, and e2e-verified by copying two leaves out
and running the README commands verbatim. **#172** — onboarding
drift batch: all 13 audit items closed — AGENTS.md's dead `nros build`/`deploy` verbs,
the `examples/threadx-riscv64/` path (→ `qemu-riscv64-threadx`), cli.md's missing
`generate-rust`/`generate-px4-msgs`/`codegen` entries + false "no release verb" claim,
README prerequisites (ROS 2 + cmake now stated required) + the ros-launch-manifest
submodule init added to every cargo-build route, bootstrap routes unified across
README/cli.md/activate hints, `nros sync` added to `nros --help`, and
`nros setup --list`/`--licenses` moved to stdout (pipeable). **#169** — book config
sweep: 15 pages still taught the retired per-example `nros.toml`/old-`config.toml` model with
404 links; `configuration.md` rewritten around RFC-0004's live model (`deploy` metadata /
`nano_ros_deploy` + `system.toml` + the kept direct-mode `config.toml` for no-codegen `no_std`
apps), every embedded starter + first-node page re-grounded on the shipped manifests, and the
fixture-port vs copy-out-port split documented. **#168** — zenohd
split-brain: nine `just` recipes invoked bare `zenohd` that no setup route puts on PATH; a
shared resolver (`scripts/dev/zenohd.sh`, build/zenohd → SDK store → PATH) now backs every
`just <plat> zenohd` recipe, and README/examples docs converge on that one launch line.
**#158** — the NuttX/native
realtime tier e2e now prove tier ordering deterministically: each tier publishes a monotonic
counter and the assertion compares highest-delivered VALUES (`ctrl_max >= 3 * telem_max`) —
timer-fire progress, immune to delivery batching/drops — replacing the count heuristic and the
jitter-prone `wait_for_output_count` gate. **#163** — pure-Rust
Zephyr images carry the zenoh/xrce backend again (real optional deps + a force-link register
call in `zephyr_component_main!` past staticlib DCE + the picolibc malloc-arena bump + an
XRCE `host:port` locator bake); rust zenoh AND xrce pubsub/service/action all green — the
zenoh lane's first pass since the phase-248/249 registration rework. **#162** — w1d tier
probe: gated the measurement on a first delivery (retry the boot once on the gossip-gap race,
then fail loud) + `max+1` denominator + an IDEAL verdict case (clean fixtures read 1500/1500 =
100 %, corroborating #148). **#102** — example
fixture coverage: phase-284 reconciled the stale 07-01 inventory and drove it to resolved —
covered (H1 phase-276; H2 entry build-asserts + nuttx/freertos runtime; H3 custom-msg + logging
+ rust async e2e) or de-scoped-with-reason (cpp POCs proven by the cpp workspace entry e2e;
non-Zephyr embedded matrix fill; cyclone-RMW svc/action as secondary-transport matrix; embassy
listener redundant demo). No silent caps. **#161** — the 177.37
domain bake was defeated by two later regressions: phase-180's separate
`CONFIG_NROS_CYCLONE_DOMAIN_ID` knob pinned 0 everywhere (now defaults to `NROS_DOMAIN_ID`,
20 pins dropped) and the phase-277 macro rework dropped the Rust-side `NROS_DOMAIN_ID`
consumption its build.rs comment promised (restored); images bake domains 50–58 again,
group back to `max-threads 4`, phase_118 8/8 ×3 parallel in ~6 s (was ~23 s). **#160** — hand-mirrored
FFI structs now have two drift gates: buildless field parity
(`check-ffi-struct-mirrors`, push lane) + a cross-include TU in `check-c` that lets the
compiler flag prototype/typedef divergence ("conflicting types"); both verified against
seeded drift. **#159** — the missing
NuttX-ELF backstop turned out to be clobber-reverted (`f344492e4`) — restored, together with
the last other clobber loss (rust_nuttx_entry_e2e String prefix, `791677222`); the custom
command now also verifies the kernel ELF itself (two layers); fallout fixed en route: a
clang-format-corrupted `@NROS_ENTRY_PKG_SYM@` entry template (+ `.clang-format-ignore`) and
the `component.h` QoS mirror missing `tx_express` (by-value ABI mismatch, #131 class). **#136** — example
naming drift: the mechanical sweep (items 1–3 — `TalkerNode`→`Talker`, Zephyr C++ namespaces
→ `<plat>_cpp_<case>`, per-platform `setvbuf` uniformity) landed + verified in phase-283
(Complete); item 4 (`_entry` rename) → phase-275, item 5 (dup ids) already resolved. **#110** —
per-entry executor callback-table sizing: resolved by phase-271 (`Executor<'s>` borrows
caller-owned storage; codegen derives size from `CALLBACK_COUNT`, `nros::main!` reads per-entry
`max_callbacks` → `open_sized`). **#149** — nuttx-realtime typed-C fixtures (archived).
**#148** — the 100 Hz
ctrl tier's "~20% tx drop" does not reproduce on cleanly built fixtures: zero loss at line
rate (1498/1498, deterministic across 10 runs, same fork); the morning's 80% was measured on
incremental objects straddling the W3 `tx_express` struct append (the #150 stale-mixing build
state), and the garbage-`tx_express` mechanism was explicitly refuted (forced express still
delivers 100% on native). **#157** —
zephyr-cyclone C/C++ services: delivery worked once the descriptor registry accepted ROS-form
type names (`ros_form_to_dds`); the residual "never delivers" was two stale test markers
(`Result:` / `[OK]` — neither client prints them; both → `SERVICE_RESULT_PREFIX`), plus the
`zephyr-native-cyclonedds` nextest group serialized (all images bake domain 0 → SPDP collisions
until the per-role-set domain bake returns) and a `nros_c_qos_default()` `tx_express` garbage
init; phase_118 8/8 across three consecutive runs. **#156** — nuttx
logging-smoke "boots silent": a `bins/` resolver profile mismatch — `build_test_fixture` looked
in `nros-fast-release/` while the NuttX build writes `release/` (lto=on, to dodge the
`nros-fast-release` cross-CGU miscompile that IS the silent boot); forced `release` for the nuttx
target (the image itself prints all six severities). **#155** — zephyr-cyclone
silence: west-update-reverted zephyr-tree patches + pure-Rust images never registering a
backend since 248/249 + silent-return masking + phase-271 heap sizing; boots/pubsub green,
service residual = #157. **#154** — the Zephyr
shim path migrated to the post-258 bake contract (config header + cmake mirror; stub main in
the fixture app; 6/6 tests + 3/3 west bakes green). **#152** — per-lane env
gaps (all lanes green, split to #154/#155, or handed to the phase-281 stream — whose #130
fix landed both nuttx entry e2e green; build verbs + the rmw-filter manifest gotcha recorded
in the archived issue). **#153** — ros2-server→
nano-client timeouts (missing rmw attachment on queries + liveliness-vs-queryable gossip gap +
action-test type mismatch; rmw_interop fully green). **#145** — zephyr tx
throughput ceiling (phase-282: batch + flush thread + split lock = 20× streaming, uniform
`tx_express` QoS escape; successor axis = #148). **#150** — native e2e delivery timeouts
(XRCE session-key collision pid-salted; bridge bins' Int32→String flip; safety resolver
drift; qos-mixed stale-object rebuild; 12/12 green). **#151** — rmw_interop stale skips +
latency window + action-pkg gate (residual direction = #153).

Resolved issues live in [`archived/`](archived/). Recently resolved: **#144** —
[`run_tiers` ≥3-tier setup declare
race](archived/0144-run-tiers-spawned-tier-declare-race.md): the chained-spawn fix
(`spawn_next_tier`) landed on BOTH the Zephyr and FreeRTOS `run_tiers` — each tier spawns the next
only after its own `setup()` returns, so no two entity-declare calls overlap on the shared
zenoh-pico session (covers any tier count by construction; FreeRTOS's old boot↔tier race closed
too). Verified by `realtime_tiers_zephyr_entry_e2e`. **#142** —
[stm32f4 talker dual
classification](archived/0142-stm32f4-talker-dual-classification-fails-example-shape.md): the
0100.W4 collapse is intentional (a self-dispatching Entry that is its own node); `example_shape`
now mirrors the CLI schema (`entry` MAY coexist with a node, `application` must stand alone) and
passes. **#130** —
[NuttX Entry path never configures eth0](archived/0130-nuttx-entry-init-hardware-noop-no-eth0-config.md):
both the Rust and C/C++ entry paths now push the guest IP into `eth0` before
`Executor::open` from one shared `configure_entry_eth0` (`SIOCSIFADDR`) helper
(`703e840dd` Rust, `1f8b82d3b` C). Proven at runtime — the entry image applies
`eth0=10.0.2.30` and delivered 39 cross-process `/chatter` messages to a native
listener (pcap + listener log); the old `Transport(ConnectionFailed)` is gone.
The `rust_nuttx_entry_e2e` timeout was compounded by a wrong grep prefix
(`"I heard:"` vs Int32 `"Received:"`), fixed in phase-280 (nextest CI-lane stamp
per the phase doc's sandbox caveat). **#147** —
[Fixture staleness enforced only under `just test-all`, not at the
resolver](archived/0147-plain-example-fixtures-no-staleness-detection.md): the fixture resolvers
now carry a detect-only dep-info probe (cargo `<binary>.d` / `ninja -t deps` / the west staticlib
`.d`), so a bare `cargo nextest` hard-fails "… is STALE" naming the newer source instead of
silently running a stale binary — the recurring hazard behind #146/#129/#140. Reads the toolchain's
recorded dep graph + stat, never rebuilds (phase-278). **#146** —
[ROS 2 → nano native interop delivers
nothing](archived/0146-ros2-to-nano-native-interop-delivers-nothing.md): a TEST defect, not a
product bug — `topic_pub` hardcoded `--qos-reliability best_effort`, incompatible with the reliable
nano subscriber (a reliable ros2 pub delivers fine), compounded by a 10 s pub / 8 s window both
under rmw_zenoh's ~10 s discovery. Fixed test-side (reliable pub, 45 s pub, 25 s windows);
`test_ros2_to_nano` + matrix `case_3` green. **#138** —
[threadx-rv64 rust examples `--allow-multiple-definition`](archived/0138-threadx-riscv64-examples-allow-multiple-definition.md):
the single-runtime consolidation made the flag vestigial — dropped it from all 6 example CMakeLists
(all 6 cyclone binaries relink with zero dup-symbol errors), extended `check-no-allow-multiple-def.sh`
to scan `examples/**` + `packages/**` CMake (gate now reports zero uses), and had the fixtures recipe
build all 6 rust cyclone examples so it stays enforced. **#131** —
[ThreadX RISC-V64 lane](archived/0131-threadx-riscv64-null-c-app-main-on-rebuild.md): the C
`jalr->0` was a stale config-header mirror under-sizing `__nros_c_inst` (fixed by clean build +
a fail-loud `carve` `assert!`); the Rust TX-dead was a four-part chain — no backend registered
on bare-metal (`.init_array` no-op → explicit `nros_rmw_zenoh::register()`), `__assert_func`→stderr
link fail, no `log` sink, and duplicate zids from identical baked ip/mac. pubsub + service e2e green.
**#132** — [Rust RTOS fixture resolvers](archived/0132-rust-rtos-pubsub-fixture-resolvers-point-at-unbuilt-binaries.md):
nuttx/threadx resolvers retargeted to the bootable `*_entry` ELFs so the combos run (the
coverage-lint hardening is deferred). **#133** —
[interop soft-pass on 0 received](archived/0133-ros2-interop-tests-soft-pass-on-zero-received.md):
12 log-and-return sites in `rmw_interop.rs` converted to `assert!` (delivery is the SUT after
`require_ros2`) / `skip!` (env gaps). **#134** —
[nros-c `AtomicU64` on riscv32](archived/0134-nros-c-atomicu64-breaks-riscv32-nuttx.md):
`AtomicU32` (counter range fits); qemu-riscv-nuttx C talker builds. **#137** —
[Embedded declarative action clients were
send-only](archived/0137-embedded-declarative-action-clients-send-only.md): not a missing seam —
`create_action_client_with_callbacks_for_name` (212.M-F.23) already auto-drives
accept→feedback→result; the freertos/nuttx/baremetal-RTIC examples just used the plain send-only
builder. Switched to the with-callbacks variant + filled `on_callback`; `test_rtos_action_e2e`
NuttX/Rust green (client observes `Goal accepted` + `Result received`). **#143** —
[Zephyr per-node-liveliness gate lifted](archived/0143-lift-zephyr-per-node-liveliness-gate.md):
the #129-era gate treated a #139 symptom; reverted, all ten zephyr images rebuilt, suite green,
and `ros2 node list` now shows every per-component node on Zephyr (multi-node images previously
advertised only the primary session node). **#141** —
[nros publisher → rmw_zenoh_cpp subscriber delivers no
data](archived/0141-nros-pub-to-rmw-zenoh-cpp-sub-no-data.md): not reproducible — router debug
logs show `ros2 topic echo` subscribing on the exact keyexpr nros publishes (TypeHashNotSupported
both sides), and both rclpy and echo receive from the same image the failures were seen on; the
original observations were #139-era environmental. The real gap (zero coverage of the
nros-pub → ros2-sub direction) is closed by the new `qos_zephyr_ros2_interop_e2e`. **#140** —
[Native per-host entry (hosted spin) subscription receives
nothing](archived/0140-native-per-host-entry-subscription-receives-nothing.md): observability, not
delivery — gdb showed the full chain live (declare, 8 pushes, ring drained, `dispatch_into_cell`
×8) while `observed_callback_counts` folded only `ExecutorNodeRuntime::components`, which the
macro install seam (`register_node_borrowed`) never populates (its cells live only as the
executor's enrolled slots). Counts now fold the enrolled cells too;
`multihost_runtime_e2e` + the un-ignored 276-W6 `multihost_zephyr_entry_e2e` both green —
phase-276 complete (all six waves on Zephyr). **#135** —
[Native zenoh service/action query path
broken](archived/0135-native-zenoh-service-query-path-broken.md): a C ABI mismatch, not a protocol
bug — the 0096 loopback fix enabled `Z_FEATURE_LOCAL_QUERYABLE` in the generated zenoh config, but
`build_c_shim` compiled `zpico.c` against the in-tree fallback config, so `z_get_options_t` layouts
diverged and the library read the shim's `target=ALL(1)` as `allowed_destination=SESSION_LOCAL(1)`;
every cross-process query silently went session-local and finalized instantly with no reply. Fixed
by compiling the shim (and the net-type size probes) with `ZENOH_GENERIC` + the OUT_DIR generated
config, and deleting the stale `c/platform/zenoh_generic_config.h` shadow copy. Native zenoh
service/action suites 11/11 incl. the 0096 in-process guard. **#128** —
[`nros::main!` Zephyr/Esp32 emit branch wires only
register+spin](archived/0128-zephyr-entry-macro-no-params-tiers-lifecycle.md): both halves landed —
params/lifecycle emits (276 W1/W3) and the hard half, `ZephyrBoard::run_tiers` (one `k_thread`
per tier over one shared session, raw `[tiers.*.zephyr]` priorities, boot thread adopts tiers[0]'s;
`realtime_tiers_zephyr_entry_e2e` green: /ctrl (10 ms) outruns /telem (100 ms) cross-process).
En route: a concurrent-declare interest race (boot setup now precedes tier spawn — the losing
publisher's write filter stayed closed) and the zsock tx-throughput ceiling (~1 send per recv
window) made tunable via `CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS`. **#139** —
[Zephyr native_sim service/queryable reply path
unresponsive](archived/0139-zephyr-service-reply-path-unresponsive.md): not a reply-path defect —
the session was silently dying. Zephyr zsock serializes send/recv on a per-fd mutex, and zenoh-pico's
Zephyr `Z_CONFIG_SOCKET_TIMEOUT` of 5000 ms let the blocking read task starve every tx (declares,
lease keepalives, replies) until zenohd dropped the lease. Fork patch drops Zephyr to the 100 ms the
unix port uses; boot 29 s → ~3 s, all five REP-2002 services answer, `lifecycle_zephyr_entry_e2e`
green. Same mechanism family as the #129 liveliness-declare wedge. **#129** —
[Zephyr rust workspace-entry lane broken on current
main](archived/0129-zephyr-rust-workspace-entry-lane-broken.md): stale June prebuilts had masked a
three-layer rot. (1) executor's ~75 KiB heap alloc vs picolibc's 16 KiB malloc arena → arena bump;
(2) phase-248 C6g removed the Rust-Zephyr backend dep + registration → restored per the RFC-0031
C5b amendment (entry-owned `dep:nros-rmw-zenoh` + the `nros::main!` Zephyr arm's deploy-rmw
`register()` emit); (3) `git bisect run` converged on 6601c7e52 (268-W2b): per-entity node identity
made entity-creates fire the lazy per-node NN liveliness declare, which wedges the app thread in
the kernel per-fd lock on native_sim — per-node tokens now gated off on the Zephyr platform (the
#104 primary token stays). Lane green: C entry publishes; `params_zephyr_entry_e2e` (276 W1
params-on-Zephyr) passes un-ignored. **#126** —
[Embedded C/C++ `run_tiers` (FreeRTOS) does not
run](archived/0126-embedded-run-tiers-freertos-session-and-stack.md): phase-274 W3's embedded
RFC-0015 Model 1 now runs on QEMU mps2-an385. Three fixes — (0) the "native single-tier emit" was a
**stale `nros` CLI** (`just setup-cli`); (A) **256 KiB tier-task stack** (64 KiB HardFaulted); (B)
the session-never-connects blocker was **`spin_once(storage, 0)`** — timeout 0 never drove the
zenoh-pico handshake; passing the tier period as the spin timeout (blocking read, as `run_components`
+ the Rust path do) fixes it. Both tiers now schedule + publish at their periods (`[ctrl]` 10 ms ~6×
`[telem]` 100 ms, each tick gated on `publish_raw().ok()`). **#103** —
[C++ lifecycle had no idiomatic wrapper
class](archived/0103-cross-language-capability-surface-gaps.md): the last cross-language capability
gap. Its other two audited gaps were already closed (multi-type params — Phase 91.C/117.9; RT tiers
— Phase 110.B; the audit cited the wrong header path), and phase-269 auto-wires the declarative
param/lifecycle entry paths. The remaining gap — no `nros::LifecycleNode` — was closed by **phase-270**
(DONE 2026-07-02): a freestanding-safe rclcpp-shape base class (`lifecycle.hpp`, six `on_*` virtuals →
`CallbackReturn`) over no_std `nros_cpp_lifecycle_*` FFI shims. Verified by
`cpp_lifecycle_node_wrapper_e2e` (`managed_node_wrapper_reaches_active_and_publishes`, green). **#123** —
[`workspace-shadowing` template read the sizes-header `#error`
stub](archived/0123-shadowing-template-smoke-cpp-ffi-sizes-header-race.md): a verbatim rclcpp
consumer that pulls nros-cpp only transitively never triggered the `nros_{c,cpp}_config_header`
mirror target, so under `make all` the mirror dir stayed empty and `#include
"nros/nros_config_generated.h"` fell through to the stub. Fixed by making `nros_c-static` /
`nros_cpp-static` depend on their own mirror target, so any consumer linking nano-ros builds the
per-build headers first (4 consumer-side `add_dependencies` hooks failed before anchoring it on the
linked static lib). **#124** (phase-272) —
[rclcpp-shape C++ components weren't bound to a scheduling
tier](archived/0124-rclcpp-shape-cpp-nodes-not-sched-bound.md): dissolved by RFC-0047's unified
config-driven binding — a `node_name → sched_context` table seeded from config + looked up at the one
`node_builder(name)` site every node funnels through — so an rclcpp-shape node's ctor picks up its
tier by name, no `NodeHandle` change; proven by `realtime_tiers_cpp_rclcpp_e2e`. **#116–#119**
(phase-269) —
C/C++ entry feature parity: [params](archived/0116-cpp-c-component-launch-parameter-readback.md),
[lifecycle autostart](archived/0117-cpp-c-entry-lifecycle-autostart-codegen.md),
[subscription integrity](archived/0118-cpp-c-component-subscription-integrity-readback.md),
[scheduling tiers](archived/0119-cpp-c-entry-scheduling-tiers-codegen.md) now project from the Rust
`nros::main!` surface onto the C/C++ entry codegen (one shared foundation + a wave each), verified by
the `cpp_c_*`/`realtime_tiers_*` e2e across C + C++. **#120** —
[bridge-workspace fixtures fail when the cyclonedds submodule is
absent](archived/0120-bridge-workspace-fixtures-fail-when-cyclonedds-submodule-absent.md): the
`workspace-rust-native-bridge` leaf built anyway and died with a cryptic `E0433` instead of
honoring its cyclonedds-submodule gate. Fixed with an explicit dependency gate in
`workspace-fixtures-build.sh` (native cyclonedds rows fail LOUD + actionable when
`third-party/dds/cyclonedds` is absent — the bridge vendors C++ CycloneDDS by design; the gate
checked the wrong stale path `third-party/cyclonedds` until phase-263 follow-up). Also: **#121**
(resolved — not a bug) — [`workspace-rust-threadx-linux` E0463 was target-dir pollution, not feature
unification](archived/0121-threadx-linux-entry-nros-platform-host-unification.md): a pristine
cyclonedds-provisioned `build-test-fixtures` builds the leaf green (`== threadx_linux == OK`), and
`nros-platform[platform-threadx]` does produce a usable host rlib. The E0463 only appeared with
mixed-`--target` artifacts left in the shared `target-fixtures/threadx-linux` by ad-hoc builds; no
CI pollution vector exists (threadx-linux isn't in `NROS_FIXTURE_SHARED_PLATFORMS`). Fix is `rm -rf`
the target-dir, not a code change. Also: **#122** —
[threadx-rv64 Cyclone message-lib TUs raced the `nros_c_config_header`
mirror](archived/0122-threadx-rv64-message-lib-sizes-header-race.md): the 0088/0090/0114
sizes-header race recurred on the threadx-qemu-riscv64 Cyclone fixtures because the 0114
`OBJECT_DEPENDS` fix was gated `NANO_ROS_PLATFORM==posix`, yet threadx-rv64 uses the same Corrosion
mirror. Fixed by gating on the mirror target's existence instead of the platform name. (Surfaced once
the sibling cross-Cyclone self-provision fix let the graph compile to the message libs.) Also: **#96** —
[in-process (same-executor) node-to-node delivery did not
happen](archived/0096-in-process-same-executor-service-roundtrip-broken.md): zenoh-pico's
same-session loopback (`Z_FEATURE_LOCAL_SUBSCRIBER`/`Z_FEATURE_LOCAL_QUERYABLE`) was hardcoded
0 for every target, so two nodes of one `nros::main!` entry never talked. Fixed by enabling the
flags for host/native in `nros-zpico-build` (kept off on embedded — RAM); additive, so external
delivery is preserved. Guarded by `tests/service_roundtrip_inprocess_e2e.rs`. Also: **#105** —
[multi-node entry collapses to one graph
node](archived/0105-multi-node-per-node-graph-naming.md): resolved by phase-268 / RFC-0046 — per-node
NN liveliness tokens on the shared session (no session-per-node); root-cause fix threaded per-entity
node identity through the CFFI session view (`entity_view`, no vtable ABI change). Also: **#115**
(wontfix) — [rustc / ld crashes under heavy fixture load are caused by unstable host
RAM](archived/0115-rustc-nondeterministic-ice-sigsegv-under-fixture-load.md): looked like a
non-deterministic rustc bug, but the host kernel log shows SIGSEGV / GPF / `invalid opcode`
across many unrelated binaries (`libLLVM`, `librustc_driver`, `ld.bfd`, `python3`,
`libtorch_cpu`, even `libc.so.6`) over ~2 months — a fault *inside libc* and in read-only shared
pages means **physical RAM corruption** on the (non-ECC, Threadripper 2950X) host, not a code
defect. `wontfix` in-repo; remediation is hardware (memtest86+, disable XMP/DOCP, reseat/test
DIMMs). A retry-wrapper attempt was reverted — on corrupting RAM it masks silent miscompiles.
Also: **#113** —
[config-driven bridge endpoints not
env-overridable](archived/0113-bridge-config-endpoints-not-env-overridable.md):
`run_from_config` baked each `[[node]]`'s locator + domain with no runtime override.
Fixed (phase-267): `apply_node_env_overrides` applies `NROS_BRIDGE_<NODE>_LOCATOR` /
`NROS_BRIDGE_<NODE>_DOMAIN` over the baked config, so a deployed bridge re-points
without a rebuild and the gated test uses an ephemeral router + `unique_ros_domain_id()`.
Verified forwarding on non-baked endpoints (:7600 / domain 9). Also: **#114** —
[native C/C++ cmake fixtures race the per-build config-header
mirror](archived/0114-cpp-cyclone-fixture-build-sizes-undefined.md): the
native/posix C/C++ fixtures compiled before Corrosion's `nros_{c,cpp}_config_header`
mirror ran, reading the in-tree `#error` stub (`*_OPAQUE_U64S` undefined → cascade
`Subscription has no member storage_`) — the same 0088/0090 race on the path those
fixes excluded. Fixed (phase-267) by wiring the hard `OBJECT_DEPENDS` edge for posix
in `NanoRosEntry.cmake` (entry sources) + `NanoRosGenerateInterfaces.cmake` (the
`<pkg>__nano_ros_c` message lib); `native-cmake-rmw` now builds all four cells clean.
Also: **#112** —
[`nros-cpp` `component_node.hpp` included `<string>` unconditionally → broke Zephyr minimal
libcpp](archived/0112-zephyr-cpp-component-node-requires-string-minimal-libcpp.md): `<string>`
was gated on `__STDC_HOSTED__` (true for host `g++` even under `-nostdinc++` minimal libcpp),
but its only consumer — the `std::string`-keyed parameter overloads — is gated on `NROS_CPP_STD`.
Moved the include onto its actual consumer's gate; `<cstdio>` stays hosted. Verified: all six
Zephyr C++ XRCE entries now build to `zephyr.exe`. Surfaced after #111 unblocked the zephyr leg.
Also: **#111** —
[`nros-sizes-build` filesystem fallback searched the wrong profile
dir](archived/0111-sizes-probe-filesystem-fallback-custom-profile-path.md): the fallback built
rlib search paths from `PROFILE` (only ever `debug`/`release`), so for the custom
`nros-fast-release` profile it looked in `release/deps` while the rlib was in
`nros-fast-release/deps` → `EXECUTOR_SIZE` probe timed out → `nros-cpp` failed. Fixed with a
`profile_dir_name()` helper deriving the real profile dir from `OUT_DIR` (the component before
`build`). Verified end-to-end: the affected dev box's zephyr Rust + C fixtures now build; the
remaining zephyr C++ `<string>` failures split to #112. Also: **#95** —
[executor `MAX_CBS` overflow → opaque
`NodeRegister`](archived/0095-executor-max-cbs-overflow-opaque-noderegister.md): a topology
declaring more callbacks than `NROS_EXECUTOR_MAX_CBS` (default 4) failed as an opaque
`NodeRegister("<pkg>")` with the underlying capacity error discarded at every collapse seam.
Fixed the diagnostic half (gap A): a distinct `NodeError::ExecutorFull` threads source
(`next_entry_slot`) → `NodeDeclError::ExecutorFull` → install `-2` → the `nros::node!` register
wrapper → `RuntimeError::ExecutorFull(<pkg>)`, whose `Display` names the actionable
`NROS_EXECUTOR_MAX_CBS` knob (arena overflow keeps `BufferTooSmall`; modes now distinguishable,
regression-locked in `executor/tests.rs`). Per-entry sizing ergonomics (gap B) split to #110.
Also: **#99** —
[declarative `[[bridge]]` does not
forward](archived/0099-declarative-bridge-planner-population.md): the cross-RMW bridge
orchestration is complete + verified end-to-end — the planner emits `build.transports` +
`plan.bridges`; `nros sync` resolves topic→type via synthetic node metadata
(`[[package.metadata.nros.node.publishes]]`) → `nros-bridge.toml`; plain `nros::main!` emits
`run_from_config_str` + the backend `register()` (#106); `cargo build` links. Done in phase-267
(W0/C1–C5) + `14b7a4cc3` (synthetic type `pkg/msg/Name` namespace fix); full runtime forwarding
verified (phase-267 W-B, #107). Also: **#106** —
[RMW backend self-register ctor
dead-stripped](archived/0106-backend-self-register-ctor-dead-stripped.md): a bridge Entry
referenced no backend symbol, so the linker dead-stripped the `nros-rmw-*` crates' `.init_array`
self-register ctors → `open_multi` null vtable → `Transport(InvalidArgument)`. Fixed (`0d205c1f7`):
`nros::main!` reads the bridge's RMWs from `system.toml` and emits `nros_rmw_<x>::register()` in
the generated `main` (no per-Entry `extern crate` boilerplate). Verified via macro expansion + 4
unit tests; full runtime `open_multi` chains on #107. Also: **#107** —
[Cyclone descriptor not staged in a schema-free
bridge](archived/0107-cyclone-baked-descriptor-not-auto-staged.md): `run_from_config`'s Cyclone
egress failed `PublisherCreationFailed` (no descriptor, and `std_msgs/Int32` is NOT baked);
resolved at phase-267 W-B (fix B) — `nros sync` carries the flat field schema in `nros-bridge.toml`
and the runtime stages the descriptor via `register_type_descriptor` (self-consistent offsets,
no user build.rs). Also **#109** — [config bridge extra session ignores
`domain_id`](archived/0109-config-bridge-extra-session-ignores-domain.md): `create_node_on`
dropped the configured domain so every extra RMW participant opened on domain 0; fixed with
`create_node_on_with_domain`. Also: **#108** —
[FreeRTOS MPS2-AN385 linker omits
`.nros_boot_config`](archived/0108-freertos-linker-missing-nros-boot-config-section.md): the
phase-266 baked `.nros_boot_config` section (`8088e77c0`) overlapped `.data` because the FreeRTOS
board's hand linker `mps2_an385.ld` never placed it → `build-test-fixtures` failed linking
`qemu_freertos_entry`. Fixed (`5a6407bd2`) by adding a `.nros_boot_config > FLASH` section before
`.data` (mirroring the script's `.eh_frame_hdr` fix); `just freertos::build-examples` now builds
the entry green. **#98** + **#101** —
boot-config unification ([archived/0098](archived/0098-nros-main-ignores-component-node-name.md),
[archived/0101](archived/0101-board-boot-config-not-unified.md)): node_name/locator/domain resolved
four ways across boards → one `ExecutorConfig::resolve` path + a single `.nros_boot_config` bake
site read by Rust, C, and C++; node naming now works on all 10 boards + 3 languages (verified
`/param_talker`, `/talker`). Fixed in phase-266 (`a314b02eb` Rust, `b2c3e63f1` C/C++); residuals
split to #105. **#97** — [`nros codegen entry` embedded
runners](archived/0097-codegen-entry-cpp-native-only-no-embedded-runners.md): C/C++ LAUNCH entry
was native-only; resolved by phase-263 C2a embedded runners. **#104** —
[C entries invisible in `ros2 node list`](archived/0104-c-nodes-no-graph-liveliness.md): the ROS 2
node liveliness token was never declared on any path (`declare_node_liveliness` had zero callers),
so nodes appeared only via entity-liveliness inference — and C entries were invisible entirely.
Fixed (`194babcf1`) by threading `node_name`/`namespace`/`domain_id` `RmwConfig`→`TransportConfig`→
session and declaring + holding the node token in `ZenohSession::new`; a native C entry went from
empty `ros2 node list` to `/node` (verified). Residuals split to #105 (per-node tokens). Also: **#100** —
[baremetal standalone examples split into a sibling node
pkg](archived/0100-baremetal-standalone-examples-split-into-sibling-node-pkg.md): the
`qemu-arm-baremetal`/`stm32f4` rust examples were an Entry binary path-dep'ing + `[patch]`ing
up into a sibling `*_pkg`, breaking copy-out. Collapsed all 25 packages (23 user examples + 2
e2e fixtures) into single self-contained crates over W1–W7 (declarative, RTIC `node_pkgs`
self-reference, Embassy, shared-pkg duplication, cross-pkg placeholder inlining), and merged
the now-redundant two-pass baremetal build loop. Also: **#94** —
[`nros ws sync` line-based TOML editor](archived/0094-ws-sync-toml-line-scanner-fragility.md):
the `[patch.crates-io]` rewriter was a line scanner, not a TOML parser (duplicate table on
the quoted `[patch."crates-io"]` form; dropped patches for explicit `[dependencies.<name>]`).
Resolved at [phase-265](../roadmap/phase-265-ws-sync-config-patch-toml-edit.md) W4 — `nros sync`
writes `[patch.crates-io]` to `.cargo/config.toml` via a `toml_edit` DOM, never editing a
consumer `Cargo.toml`, so the entire A–F class is structurally impossible. Also: **#72** —
safety-e2e CRC dead over zenoh (`nros/safety-e2e` didn't reach the backend's
`safety-e2e`): fixed via the RFC-0031 capability-axis generalization (Phase 252) —
`[safety]` lowers to the entry umbrella, the board-less native backend dep, AND the
board crate's `safety-e2e` forwarding feature (gated on the board's `nros-board.toml`
`capability_features`). This pass added the forwarding feature to the last 3 zenoh
boards lacking it (embassy-stm32f4, rtic-mps2-an385, rtic-stm32f4) so the family is
uniform; 7/7 capability tests + native/declarative `crc=ok` e2e green. Residual:
optional embedded runtime e2e. See `archived/0072-*`. **#75** —
`qos_overrides` best_effort test failed on CI only (looked like a subscriber hang):
actually a test-harness output-consume race — `wait_for_output_pattern` returns its
whole read buffer on match, so the first of two sequential waits ate the later
`Waiting for` line when the listener's logs coalesced into one `read()` (deterministic
on CI's buffering, split locally). Fixed by one wait for `Waiting for` + asserting the
earlier `qos effective` line in the same buffer. host-integration 11→4→1→0. See
`archived/0075-*`. **#71** —
native cpp/mixed workspace Entry link failed on CI only: `libnros_cpp.a` + the
per-package FFI staticlib are two Rust staticlibs each bundling `std` →
duplicate `rust_begin_unwind`. Root cause = `host-integration-tests.yml`'s
`CARGO_PROFILE_RELEASE_LTO=off` overriding the FFI crate's `lto=true` (the
`panic=abort` crate relies on fat LTO to DCE-strip the redundant unwinding std;
`off`/`thin` retain it). Fixed by dropping the override on the workspace-fixtures
step (rust-core keeps it — binaries, no dup); CI-confirmed real failures 4→1. See
`archived/0071-*`. **#70** —
staticlib link-determinism gate red: the test expected the pre-D3 2-archive pair,
but #62/phase-249 landed the single-runtime model (one `libnros_c.a`, zenoh
bundled). Rewrote `staticlib_duplicate_symbols.rs` for the single archive — links
with `-u nros_rmw_zenoh_register`, NO `--allow-multiple-definition`, one `REGISTRY`;
dropped the obsolete 2-archive dup-diff. See `archived/0070-*`. **#69** —
dep-chain gate red: `dep-chain-check.sh` (1) feature-detected via a loose
substring grep that matched a dependency's requested `rmw-zenoh` feature, and
(2) ran `nros generate-rust` on package.xml-less board-driven talkers. Fixed →
own-feature detect (python) + package.xml-gated codegen; 9/9 cells pass. See
`archived/0069-*`. **#68** —
CycloneDDS ROS 2 action interop "Goal was rejected": an incomplete Phase-233.6
migration left `service.cpp::split_wire_header` re-inserting a `uint32(16)`
goal_id length prefix on the SendGoal/GetResult request receive path, which a real
`rcl_action` client never sends and the post-233.6 action core no longer reads →
`order` read 4 bytes early → out-of-range reject. Fixed by dropping the
`insert_goal_id_len_at` call (+ deleting the dead helper); `cyclonedds_ros2_interop`
5/5 PASS. See `archived/0068-*`. **#67** —
rust typed CycloneDDS publisher `PublisherCreationFailed`: phase-248 C5c removed
the `nros/rmw-cyclonedds` feature that was the sole activator of
`nros-node/__cyclonedds-link` → `cfg(rmw_cyclonedds_present)`, so `register_type::<M>`
no-op'd and the descriptor was never built. Fixed by re-exposing a marker-only
`nros/rmw-cyclonedds` (no concrete dep) + pointing 12 examples + 2 boards at it
(`custom-msg` excepted — hand-written msg, no `Message` impl). Validated: rust
cyclone talker publishes, 4 `native_api` cyclone tests pass. The action-interop
"Goal rejected" was mis-bundled → split to **#68**. See `archived/0067-*`. **#57** —
host-integration chronically red: Cause-1 fixture-build OOM (capped
`NROS_BUILD_JOBS=2×CARGO_BUILD_JOBS=2`) + post-cap residue triage (`fa2ecb60a`) +
QEMU/Zephyr exclude-leak fix. Validated locally (CI can't complete under the
multi-agent main-push cadence): builds green, 0 real failures in the
CI-equivalent set; the 5 cyclone-extras failures are CI-skipped and split out as
**#67** (rust typed cyclone publisher regression). See `archived/0057-*`. **#50** —
weak-symbol audit + checkers: SSoT allowlist + source gate
(`weak_symbol_audit.rs`) + final-image gate (`check-weak-symbols-image.sh`);
W3.1 weak-default deletion (phase-249 P4a); 155.A const-weak → weak getters.
Final close re-audited `smoltcp_init/cleanup` to optional-hook (legacy no-op
stubs; real bring-up is `nros_smoltcp` + `define_network_state!` — no strong def
exists) and fixed the #62 stub-rename allowlist drift. Gates green: source 11
files OK, image checked=20 fail=0. See `archived/0050-*`. **#62** —
D3 completion: R1 (dispatch → generated `NanoRosRmwDispatch.cmake` from
`resolve_rmw`, drift-guarded, consumed by the synth-runtime crate + top-level
link), R2 (weak `nros_app_register_backends` default deleted → missing
registration is a link error; closes #50 W3.1), R3 (triggers consolidated to
hosted `.init_array` ctor + embedded board call; linkme deleted) — all via
phase-249 + a cleanup tail (renamed the misnamed `weak_register_backends.c` →
`weak_platform_log_stubs.c`, scrubbed stale weak-no-op comments). Validated:
nros-c/nros-cpp build, cyclone `cpp_listener` links+runs, drift guard green. See
`archived/0062-*`. **#42** —
platform/std-header fragility (libc/std clashes #27/#36/#38): the class is fixed +
merge-gated (host `platform_header_matrix` + the new cross `cross_libc_precedence`
gate + the zephyr prj.conf gate; one canonical `<nros/platform.h>`; capability
SSoT). Decoupled from the linking class (#20/#62/phase-249). Fully closed — the
"centralise the libc-precedence helper" direction (C) was dropped as a non-goal
(the two-set clash is NuttX-only, one gated site). See `archived/0042-*`. **#53** —
mixed-RMW bridge stock-cyclonedds variant + cross-RMW gateway book recipe (211.I):
shipped `examples/bridges/tt-zenoh-to-cyclonedds` + an Int32 e2e
(`bridge_zenoh_to_cyclonedds`, forwards 8/8 live samples) + the
`cross-backend-bridges.md` recipe; raw publish stages the Cyclone descriptor via
`register_type_descriptor`. See `archived/0053-*`.

Recently resolved (CI infra,
2026-06-15): **#66** (renumbered from 64 — collided with the open esp32 #64) —
stale example Cargo.locks (`nros-core 0.1.0`) tripped the ABI guard + a clippy
empty-line in `nros/lib.rs`; fixed by regenerating 10 locks → 0.5.0 and reordering
the doc comment (validated via nuttx/stm32f4 builds + `check-workspace-all`).
**#65** — `check` cell red from a stale `nros/platform-posix` feature combo
(`justfile`, 248-C5c fallout) + nros-cpp clang-format drift; fixed by dropping the
removed feature and reformatting 5 headers with the CI-pinned clang-format 17.0.5.
See `archived/0066-*`, `archived/0065-*`. **#64** — esp32-c3 QEMU session-init
crashes (Load-access-fault → OOM-wipe → first-timer-fire instruction-fault): one
root class — the ~18 KB stack, starved by an oversized `.bss` esp-alloc heap,
overflowing into `.bss` along the deep zenoh-pico connect/spin path. Fixed by
OpenEth `new_in_place` (no 11 KB stack temp) + locator `.bss`-static + no_std
`CONFIG_PROPERTY_SIZE` 256→64 + esp-println `log::Log` logger + heap 96→48→16 KB
(stack ≈98 KB). Two-node `esp32_emulator` e2e GREEN. See `archived/0064-*`.

Recently resolved (phase-244):
**#49** — example source platform/RMW leakage: re-audit (all example/template
source, 2026-06 rescopes) → 0 blocking major; native/rust cleaned to Shape B (D7),
the zephyr cyclonedds FVP straggler migrated to the typed carrier (C2.1), residual
`minor` = node-lib `#![no_std]` (E4 accepted). qemu-riscv64-threadx → phase-245.
See `archived/0049-*`. **#60** —
platform/RMW-agnosticism audit closed by phase-248 (all four fix-path tiers
converged: cyclone vtable seam, platform cfg → vtable, boards' concrete RMW
optional, `platform-*`/`rmw-*` features retired from `nros`/`nros-c`/`nros-cpp` +
every example/fixture/codegen; embedded runtime-green on freertos/threadx-rv64/
nuttx/baremetal). The SOURCE-layer sibling **#49** + the registration-trigger
**#62**/phase-249 remain. **#61** — zephyr cmake feature remediation closed
`wontfix` (premise void: C3.2 was superseded by 241.D3, so the features remain on
`main`). See `archived/0060-*`, `archived/0061-*`. **#63** —
native Rust cyclonedds binaries dropped the posix platform C port (undefined
`nros_platform_wake_*`): `nros-rmw-cyclonedds-sys` had no `nros-platform` dep, so
nothing re-anchored the cffi rlib's `#[used]` force-link static (zenoh anchors it,
cyclone didn't) → the posix C port was DCE'd. Fixed by mirroring zenoh's
`platform-posix` feature + `__FORCE_LINK_PLATFORM_CFFI` static on the sys crate
(`de85cadc2`). Verified 2026-06-15: native cyclone Rust talker links clean. See
`archived/0063-*`. **#35** —
the 13 zephyr native_sim e2e failures were four distinct root causes, not load
flakes: 9 XRCE (`xrce_session_drive_io` looped on the wall-clock stub
`nros_platform_time_now_ms` returning 0 → switched to monotonic
`nros_platform_clock_ms`), 1 zenoh pubsub (test/example readiness markers), 2
rust service/action (the single-node `ExecutorNodeRuntime` had no service/action
dispatch → Phase 212.M-F.23), 1 cyclonedds (`__register_linked_rmw()` had no
`rmw-cyclonedds` branch → `Executor::open` returned `NoBackend` on linkme-blind
targets). 13/13 green. See `archived/0035-*`.

Recently resolved (Phase 239):
**#39** — C++ `init_with_launch_auto` null-locator env-fallback (fixed in the
3-arg `init` overload); **#40** — C++ action callback truncated result (a symptom
of #39 + a latent result offset 8→5); **#43** — C++ action server empty result
for a C-framed goal (a stale pre-233.6 C fixture writing a removed GoalId
sequence prefix; resolved by a fresh build); **#45** — FreeRTOS Entry-pkg
build/panic-handler (Component → rlib-only + board-owned `panic_semihosting` +
`mps2_an385.ld`); **#46** — FreeRTOS Entry-pkg stack-overflow at Executor
(app-task stack 256→384 KiB + zenoh heap 512 KiB→2 MiB; runtime gate un-ignored +
green); **#48** — FreeRTOS Entry firmware never connected over zenoh: the zenoh
RMW backend was never linked/registered (→ `NoBackend`) and the deploy
locator/ip/gateway was inert (`Config::default()` `192.0.3.x`). Fixed by linking
+ registering the backend (`nros/rmw-zenoh` + `__register_linked_rmw()` on
`target_os = "none"`) and threading the deploy block into the boot `Config` via
`BoardEntry::run_with_deploy` + `DeployOverlay`; `freertos_run_plan_runtime` now
asserts the connected run. See `archived/0039-*`, `archived/0040-*`,
`archived/0043-*`, `archived/0045-*`, `archived/0046-*`, `archived/0048-*`.

Recently resolved (Phase 243): **#48 (nuttx)** — the NuttX link dropped the whole
`nros_platform_*` ABI (undefined refs from `libnros_rmw_zenoh` / `libzpico_sys`).
Root cause was NOT the typed carrier (original diagnosis corrected): the board
crate's `cc` platform-port build emitted the default `static=` (`+bundle`), folding
the port into `libnros_board_nuttx_qemu_arm.rlib`, which precedes the referencers on
the link line ⇒ single-pass `ld` drops it. Fixed in `nuttx_platform_build.rs` with
`cargo_metadata(false)` + a hand-emitted
`static:-bundle,+whole-archive=nros_platform_nuttx` (trailing, order-independent).
See `archived/0048-nuttx-typed-carrier-link-drops-platform-port.md`. (Note: id 48
is shared with the earlier resolved FreeRTOS-slirp issue — a pre-existing numbering
collision.)

Recently resolved (Phase 240.5): **#47** — C/C++ action client now callback-based
(`nros::bind_action_client` = `set_callbacks` + a poll-timer pump per RFC-0041);
NuttX cpp+C action E2E green in QEMU. See `archived/0047-*`.

**#44** — esp-idf `platform.c` compile failed: esp-idf riscv `FreeRTOSConfig_arch.h`
uses linker symbols `_heap_start`/`_heap_end` (`&_heap_end - &_heap_start`) this TU
never declared. Fixed by declaring them `extern int` (matching esp-idf), gated to
`ESP_PLATFORM`, before `<FreeRTOS.h>`. Verified: esp32c3 `platform.c.obj` compiles.
See `archived/0044-*`.
