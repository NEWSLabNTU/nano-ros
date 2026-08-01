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

**#371** [severity **high**] — native_sim cyclone app abort()s at a near-deterministic 19–21 s
joining the full Autoware graph during the safety-island demo's scenario init (7/7 on 2026-08-01;
the same tree passed 2× on 2026-07-31 when one sim node was down). mrm_handler flaps
operate/cancel (hot service path) right before death; unnamed cyclone pthread; gdb masks it;
strace -k unwinds only to the zephyr print shim. Isolation: island alone / single-peer feed /
availability flap / idle sim / EKF-odometry all survive — only the full graph + scenario churn
kills it. See `0371-*`. (2026-08-01)

**#370** (filed as #368; renumbered — fifth id collision between parallel sessions) — zephyr fixture family broken after `just setup-cli` on current main: `nros codegen entry`
rejects ws-realtime-c's committed system model with "places no nodes on board `zephyr`" — the
rebuilt CLI (ros-launch-resolve line, RFC-0060) and the committed model disagree about
execution.deploy targets, so `build-test-fixtures` fails and tier-1 ci can't reach fixture
freshness. Same shape as archived #0361. See `0370-*`. (2026-08-01)

**#367** — `CONFIG_NROS_CYCLONE_CONFIG_XML` is declared in `zephyr/Kconfig` but consumed NOWHERE:
`session_create` picks env `CYCLONEDDS_URI` or the hard-coded `kEmbeddedCycloneConfig` only — and on
native_sim picolibc `getenv` sees no host environment, so the baked profile (multicast off, index
scan 0..20, no tracing) is effectively immutable on the platform that most needs tuning. Found
restructuring the safety-island demo onto Autoware's domain (island must scan ~40+ participant
indices; failure untraceable with the config sealed). Fix direction: wire env → non-empty Kconfig
XML → baked profile in `session.cpp`; single-quoted XML attributes keep the blob Kconfig-safe.
See `0367-*`. (2026-07-31)

**#368** — `just setup all` simulated end-to-end on a clean Ubuntu 22.04 host: 7 of 18 modules fail,
nearly all on prereqs the RFC-0014 index model was meant to absorb. Biggest: ONE sudo `apt-packages`
step ordered first in the workspace module aborts its own sudo-less installers (ninja/make/targets/
cargo-tools), cascading into zephyr/esp32/px4. Plus: doctor remedies pointing at apt where index
prebuilts exist (riscv gcc, idlc, play_launch_parser), the prebuilt qemu dist needing an
undeclared system `libslirp0`, the bundled interface set missing the three packages the repo's own
examples need (and a failed sync NARROWS a tracked leaf patch table — 0363 shape), pyo3/z3 build
deps undeclared, verus unpinned-latest needing glibc 2.39. Full inventory + consolidated apt line +
suggested work order in the issue. See `0368-*`. (2026-08-01)

Recently resolved: **#364** — `<node machine=>` is ROS 1 roslaunch syntax; ROS 2 rejects it, so the
phase-211.F multi-host partition was built on a fiction and the four `multihost.launch.xml` fixtures
could not be run by `ros2 launch`. phase-326 moved the partition to RESOLVE time: a standard `host`
launch `<arg>` + `if=` conditions produce committed per-host models (`multihost_robot<N>_model.yaml`,
binding recorded in `meta.args`, replayed by `nros sync`); `machine=`/`Deploy.host` removed across the
fork chain (parser → rlm → resolver), and `Plan::for_host` / `--host` / `nros::main!(host=)` / cmake
`HOST` deleted with loud migration errors. Found on the way: the `nros-launch-resolve` helper never
forwarded `KEY:=VALUE` bindings to the parser (vestigial positional swallowed the first one; binding
stopped at model metadata) — `<arg>` overrides were inert through it. See `archived/0364-*`. (2026-07-31)

**#367** — RESOLVED: the `nros ws sync` ghost (phase-265 renamed the verb to `nros sync` but a hidden
`cmd/ws.rs Sub::Sync` alias, the misnamed `nros_require_ws_sync` helper, and hundreds of stale prose refs
survived). Deleted the alias (`nros ws sync` now errors), renamed the helper, swept every active code +
user-facing-doc ref to `nros sync` (log prefixes, just recipes, book, `pr-checks.yml` job name, scripts,
examples — historical records left intact), and added a class gate (`RETIRED_SPELLINGS`, `\bws sync\b`) to
`check-retired-submodule-refs.sh` (in `check-fast`) so it can't creep back. See `archived/0367-*`. (2026-08-01)

Recently resolved: **#369** (filed as #368; renumbered) — Mixed C+C++ workspace fixture link-fails: the variant anchor suffixed by cargo FEATURE SPELLING diverged between the two nros-c builds of a mixed workspace though their SIZES agreed. Fixed: the anchor now hashes the header's own size values (`sz_<fnv64>`) and the archive def is WEAK, so agreeing builds merge and a different-sized header still fails to link. Verified on the exact failing command. See `archived/0369-*`. Original finding: `libstd_msgs__nano_ros_c.a` references
`nros_config_variant_alloc_..._rmw_cffi_rmw_zenoh_ros_humble_std`, but `nros-c` (built for that workspace
with `cffi-zenoh-cffi` → `rmw-zenoh`, NOT `rmw-cffi`) never emits it. The C-side msg codegen stamps its
variant from the WORKSPACE feature union (which carries `rmw-cffi` via the C++ half's `rmw-zenoh-cffi`),
while the linked `nros-c` staticlib carries only its own features → undefined reference. Same variant-
consistency class as #360, unreconciled for the mixed workspace where C/C++ halves pick different rmw
spellings. Fix: compute the C-side variant reference from nros-c's OWN features, not the union. Blocks
`native,mixed,*` fixtures. See `0369-*`. (2026-08-01)

Recently resolved: **#349** — the issue-0332 vtable completeness gate required three OPTIONAL capability
slots, so the xrce backend could not register at all. Split required (core transport, `.expect()`ed on
dispatch) from optional (typed `Unsupported` at the point of use), and gated the 1:1 correspondence
both ways. See `archived/0349-*`. (2026-07-29)

Recently resolved: **#348** — zpico supports only ONE zenoh session per process: `g_session` plus 51 file-scope
statics (every registration table) are process globals, and the ~38 `zpico_*` entry points take no
session handle, so multi-session means a breaking ABI change across 51 consuming files. Split out of
#347, which made the second open FAIL loudly rather than silently wiping the first session. Nothing
in-tree needs it — the bridge workspaces pair zenoh with a DIFFERENT backend — so this is a
capability gap, not a live break. See `0348-*`. (2026-07-28)

**#353** — a declared `param_services`/`lifecycle` NEVER reaches the C/C++ cargo feature list. The
bake writes `set(NANO_ROS_FEATURES "param_services")` into `system_config.cmake`, nothing includes it
on the workspace path, and the cache reads `NANO_ROS_FEATURES:STRING=` — so `nros-c`/`nros-cpp`
compile without it. Nobody noticed because the `posix` always-on in both CMakeLists IS the lowering:
on hosted it is the only path those two axes ever take, so a workspace that forgot to declare is
indistinguishable from one that did. Found by phase-315 W4 checking the removal first — the check
said safe (both real callers declare), and removing it still broke `ws-params-cpp` with `undefined
reference to nros_cpp_get_param_integer`, which is what exposed the disjoint paths. Reverted; main
unaffected. Same shape as #0311: one axis, two sources that cannot disagree because only one is
read. See `0353-*`. (2026-07-31)

**#363** [severity **high**; fix A landed] — a STALE `nros` binary silently emits a WRONG
`[patch.crates-io]` table. Its hardcoded crate→path table predated phase-321's package move, so
`nros-zephyr-build` resolved to a path that no longer exists and was **dropped without a word** —
and a dropped patch entry resolves that dependency from crates.io instead of the checkout, failing
nowhere. My first diagnosis ("a generator that writes before it can finish", fix = atomicity) was
**wrong**: the writes are already atomic (temp+rename) and complete *before* the later failure; the
output was a complete write of wrong content. The real defect is that a staleness guard EXISTS and
is good (`cargo.sh:149`) but lives in `nros_cli_bin()`, so it only runs via `just` — while
`activate.sh` puts the raw binary on PATH, so the documented recovery (`nros sync`) never reaches
it. Fix A landed: sync now refuses to emit a table omitting a managed crate with a dead path
(safe because all 23 lookup paths are in-repo with tracked manifests). B (guard on the direct path)
and C (couple the CLI to `nros-launch-resolve`, built by a separate recipe) remain.
See `0363-*`. (2026-07-31)

**#365** — RESOLVED: FreeRTOS board build couldn't find `nros/app_config.h` — `nros-board-mps2-an385-freertos/build.rs`
still joined the pre-phase-321 `core/nros-c/include` after nros-c moved to `packages/api/nros-c` (`7e3e15b4d`);
the threadx-linux board was updated, this one missed. Fix: `core`→`api` include path + a fail-loud existence
assert. Verified: `just freertos build-fixtures` no longer errors "No such file". The build then hits a
SEPARATE cmake ABI-detection failure (`undefined reference to _write/_sbrk` — newlib syscall stubs /
`--specs=nosys.specs` missing), a distinct third freertos-chain blocker (codegen → include-path → cmake-ABI).
Board-crate v0.4.0 lockstep drift left as its own question. See `archived/0365-*`. (2026-07-31)

**#366** — RESOLVED: FreeRTOS cmake configure failed *"Detecting C compiler ABI info - failed"* —
`arm-freertos-armcm3.cmake` set `CMAKE_C_COMPILER_WORKS TRUE` but not `CMAKE_TRY_COMPILE_TARGET_TYPE`, so
ABI-detection `try_compile` LINKED an exe against arm-none-eabi newlib whose `_write/_read/_sbrk/…` syscall
stubs were unresolved → configure aborted. Only freertos tripped it (nuttx=own libc, threadx=picolibc). Fix:
`set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)` (compile-only ABI detection). Verified: configure passes,
build proceeds to compilation. Third freertos-chain blocker after #361 (codegen) + #365 (include path). See
`archived/0366-*`. (2026-07-31)

**#362** — phase-325 W3's uORB→RMW bridge is blocked on TYPES, not plumbing. The plumbing is proven
(one PX4 module links uORB + zenoh, `NodeBuilder().rmw()` gives two sessions, backend selection is
the cargo-feature knob one layer down). But the bridge must TRANSLATE: inward a payload is the PX4
struct keyed by `ORB_ID`, outward a real ROS 2 subscriber needs CDR with a type name AND type hash.
`nros generate-px4-msgs` already emits exactly the right message set — CDR `px4_msgs::msg::*` from
the PX4 `.msg` tree — but as a **Rust crate**, for the XRCE companion path; an in-firmware module is
C++. Hand-rolling the CDR is not the shortcut it looks like: `rmw_zenoh` keys discovery on the type
hash, so a guessed one is either invisible to ROS 2 or, worse, decoded as a different type. Worth
doing beyond the bridge: using `px4_msgs` makes the bridge's ROS-2-facing contract identical to what
`uxrce_dds_client` already publishes, so a subscriber cannot tell which produced a sample — that
indistinguishability is the interop claim. Blocks W3 only; the W2 direct demo needs no CDR at all,
which is the point of it. See `0362-*`. (2026-07-31)

**#360** [severity raised to **high**, 2026-07-31] — DEMONSTRATED, not hypothetical: building
`libnros_cpp.a` with `rmw-zenoh-cffi` for phase-325 W3 overwrote the `rmw-cffi` archive W2's uORB
example links, and that example's SITL build failed with **74 undefined references** to zenoh-pico
platform symbols it never asked for (`BACKENDS uorb`, no zenoh use). Rebuilding with the original
features restored it. So the single-path problem is not confined to the generated header — the
ARCHIVE has it too, and feature selection changes what the archive REQUIRES, not merely what it
describes. The link failure is the lucky half; the header half fails silently at runtime. Original
finding: `nros_config_generated.h` is written to a FLAT path while its own checked-in stub
documents `<variant_slug>/nros/…` ("sorted underscore-joined cargo feature list"). One file per
project, not per feature set — so a second `cargo build` with different features OVERWRITES the
header a first archive's consumer compiles against. That header carries STORAGE SIZES, so the
disagreement is a silent runtime overflow, the issue-0268 family. `check-sizes-header-mirrors.sh`
does not cover it: it verifies each build tree's mirror matches its source, not that the source
matches the archive being linked. Found wiring phase-325 W1.4, where the stub's documented
`-I…/<slug>` cannot be followed because no slug dir exists. See `0360-*`. (2026-07-31)

**#356** — RESOLVED: removed. Reading the body before deleting it found a second, worse defect than
the one filed — both its endpoints were NANO-ROS modules (`nros_listener` + `nros_talker`, asserting
`recv: ts=`), so even with the retired examples restored it asserted nano-ros can read its own
publication. On uORB the payload IS the PX4 struct, so the interesting failure is a layout
disagreement with `orb_metadata`, and a loopback is satisfied identically by a correct and a broken
encoding. It measured the harness, not the interop it was named for. `just px4 test-sitl` now runs
Track B only and can pass; Track A is build-only via `build-sitl-cpp` and every doc says so.
phase-325 W2 supplies the replacement, whose far end is a STOCK PX4 consumer. See `0356-*`.
(2026-07-31)

**#356 (original finding)** — `px4_e2e.rs` builds SITL against `examples/px4/rust/uorb/{talker,listener}`, a tree
phase-277 W7 retired — and whose retirement is recorded in prose in `just/px4.just`, 40 lines from
the recipe that was migrated. The test asserts honestly (`assert!`, no silent pass), so nothing
reads green; `just px4 test-sitl` simply cannot pass. Filed rather than fixed: the honest repairs
are deleting it, or phase-316 W4.2 (a real uORB example), and W4.2 is blocked on deciding what a
nano-ros uORB example is FOR given PX4 ships `uxrce_dds_client`. Retiring a path is a sweep, not an
edit. See `0356-*`. (2026-07-31)

**#315** — RESOLVED by phase-316 W1–W3: all three `<rmw>/` levels gone, `check-example-matrix.sh`'s
allowlist **empty**, its `is_allowed()` px4 branch deleted. Two of the three named no RMW, so they
were RENAMED, not flattened — `px4/rust/xrce/` → `px4/rust/companion/` (where the code RUNS; its
RMW is pinned by `uxrce_dds_client` and was never a choice), `zephyr/*/cyclonedds/talker-aemv8r` →
`zephyr/*/talker-aemv8r` (a board, already said by the suffix). The third, `px4/cpp/uorb/`, held one
link-check module that is not an example at all → `packages/testing/nros-px4-register-check/`,
dissolving a hoist+shim pair that existed only because PX4's required layout and the example tree's
required layout could not both hold. See `0315-*`. (2026-07-31)

**#354** — `fixtures-manifest.py`'s `validate-workspaces` / `validate-compile-checks` had **no
caller** (`git grep` returns the script's own usage text and dispatch, nothing else), so they
decayed unwatched until 74 of 86 workspace rows failed — every one on CHECKER staleness, not a
broken fixture: the entry-target detector never learned RFC-0048's `nano_ros_add_executable` (47
rows, literally #350's shape one layer over), and `[system].default_launch` stayed mandatory after
phase-296 R4 retired the launch bake (16 rows). The remaining 11 were genuinely missing entry
`package.xml` files, written rather than excused. Both validators fixed, then wired into
`check-fast` as `check-fixtures-manifest` (buildless, ~0.1s for 112 rows). The lesson worth
sweeping for: **a validator with no caller is not coverage** — its staleness debt grows with every
verb migration and is discovered only when someone finally tries to switch it on.

**#355** — RESOLVED: CycloneDDS ros2→nano interop 0-delivery (`cyclone_pubsub_ros2_to_nano` +
`cyclone_service_nano_server`) while nano→ROS 2 TX delivered. Root cause was NOT the wire — the C
executor's blocking spin loops (`nros_executor_spin`/`spin_period`) detected a "dead session" by
counting `spin_some`'s idle `NROS_RET_TIMEOUT`, so a healthy C listener that idled >16×100ms≈1.6s
before its publisher was discovered (normal DDS SPDP) got killed. Zenoh interop uses a Rust
listener, so it never hit it. Fix: both loops now gate on the REAL `Executor::session_io_failures()`
counter (genuine `drive_io` errors only, resets on idle — issue 0324's correct signal) instead of
idle timeouts. `interop_e2e` cyclone 6/7/8 + full 10/10 green; regression test
`idle_spins_never_raise_session_io_failures`. See `archived/0355-*`. (2026-07-31)

**#361** — RESOLVED (filed as #356; renumbered after a concurrent px4 issue took 0356 — commits reference
it as #356): grandfathered committed SystemModels + a multi-board placement gap. `984fc15` stopped
`kind="embedded"` blocks partitioning (fixing re-resolve), but multi-board nodes were then pinned to the
self-block's `linux` target, so `codegen entry --board <embedded>` dropped them → c/cpp/mixed embedded
entries failed *"places no nodes on board"*. Fix (fork chain: rlm `92c1a52` → ros-launch-resolve `69c13d2`
→ nano-ros `f760141ef`): multi-board nodes are board-agnostic (`target=None`); `keep()` admits a `None`
node on every board. Part 2 (`2ce930e39`): authored the c/cpp/mixed embedded deploys + regenerated every
multi-board workspace's model. Verified `just freertos build-fixtures` codegen no longer errors "no nodes
on board" (build then hits a SEPARATE `nros/app_config.h` / board-v0.4.0 blocker). Red herring: `nros ws
sync` uses the standalone `nros-launch-resolve` binary (`setup-launch-resolve`, not `setup-cli`). See
`archived/0361-*`. (2026-07-31)

**#357** — Tier 1's FIXTURE gate scopes to native correctly (a tree with no ThreadX/NuttX/FreeRTOS/
Zephyr build dirs at all runs `just ci` without one complaint about them — measured), but its TEST
selection does not. `lane-filter.sh` excludes per-BINARY (`not binary(~threadx)`), which only works
when a platform's tests live in a binary named after it; the matrix consumers do the opposite and
put every platform's cases in one generically-named binary. `rtos_e2e` is entirely cross-platform
and matches no token at all. Measured on a 2026-07-31 tier-1 run: **53 of 88 distinct failures were
cross-platform tests the lane should never have selected**. The anti-rot test
(`lane_filter_tokens_cover_every_non_native_platform`) passes and always would have — it asserts the
TOKEN list is complete, never that the SELECTION is, so it is narrower than the rule it enforces
(the issue-0196 class, in the work that introduced the rule; filed by the author).
**RESOLVED 2026-07-31** — the filter now also emits a grouped test-level exclusion,
in both spellings (nextest `~` is case-sensitive; rstest writes `Platform__Freertos`, hand-rolled
matrices write `case_05_zephyr_rust`), with a `test(~tests::)` exemption so host-only unit tests that
merely mention a platform are not dropped. Measured by `cargo nextest list`: selection 1360 -> 1263,
97 newly excluded, all 97 naming a non-native platform, and all 53 cross-platform failures from the
acceptance run deselected. The gate that let it through now asserts test-level exclusions too, and
both new gates were negative-tested — reverting the fix makes them fail, which the original never
could. See `0357-*`. (2026-07-31)

**#359** — 24 of 49 tracked leaf `Cargo.lock` files cannot satisfy their own manifest
(`cargo metadata --locked` refuses them). Not cosmetic drift: `nros-board-nuttx-qemu-arm`
regenerates with **86 packages added and 0 removed**, so the manifests grew dependencies and the
locks never caught up. Regenerating today does not restore anything — it pins 86 registry crates at
whatever resolves at that moment, which is why a bulk refresh was deliberately NOT done alongside
`e2cc5d91d`. Inspected by regenerating each and classifying the diff: **6 PATH-ONLY** (local path
deps only, safe) vs **18 REGISTRY** (a crates.io package enters or moves — a real dependency change,
and 12 of the 18 are board or driver crates). Meanwhile nothing runs `--locked` over these leaves,
so the locks are not pinning what gets built and two builds of the same commit can differ — issue
#182's class, one layer out. Found while running tier 1 for phase-318 acceptance, which regenerated
three of them as a side effect (fixed in `e2cc5d91d`).
See `0359-*`. (2026-07-31)

**#346** — `borrowed` now works on srv/action payloads in both languages, so
**all three RFC-0033 storage modes are supported end to end** (owned / heap #344+#345 / borrowed
#346). The design question this issue raised — "a borrowed response has nothing to borrow from" —
was wrong: every payload has an owned WRITE side and a raw-buffer READ side, and the client reads
`response` bytes just as the server reads `request_data`, so the view lifetime matches a
subscription's in both directions. View macros added to the shared arm files; per-payload
`has_borrowed_*` flags; a new `gcc -Werror` check that caught a missing `nros/borrowed.h` include
(same class as #345's missing `platform.h`). Owned output byte-identical.
See `archived/0346-*`. (2026-07-28)

Recently resolved: **#345** — C `heap` now works on srv/action payloads. **The issue's own premise
was wrong**: it claimed every C consumer would need teaching to `_fini`, but `nros_service_callback_t`
hands the callback RAW BYTES — nros-c never builds a typed payload struct, so the caller already owns
it exactly as for messages. No framework, consumer, or RFC change needed: shared C arms
(`_c_field.jinja`) + a generated `_fini` per payload + the `nros/platform.h` include the change
initially missed (caught by a new `gcc -Werror` check). Owned output purely additive. Bonus: the
pre-existing `generated_heap_c_message_compiles` failed the first time it was ever run — its stub
`cdr.h` predated phase-303's DHEADER seam and nothing runs `--ignored` (#328); repaired, so message
heap is now verified too. See `archived/0345-*`. (2026-07-28)

Recently resolved: **#344** — RFC-0033 `heap` now works on **Rust** srv/action payloads. The
divergence turned out to be only the DESERIALIZE arm (Rust serialization is already container
agnostic), so one shared macro — `templates/_nros_field.jinja`, imported by the message, service
and action templates — was the whole fix. Proven output-preserving against a 10-file golden corpus
(byte-identical but for an intended string-seq convergence no committed file carries). The blanket
#343 rejection became a per-language policy table; 8 tests, including the defect inverted into a
gate: struct type and deserialize body must agree. C heap and `borrowed` stay rejected with reasons
→ #345. See `archived/0344-*`. (2026-07-28)

Recently resolved: **#343** — RFC-0033 storage modes were resolved for srv/action but implemented
only in the MESSAGE templates, so a heap-configured `.srv` field emitted the heap TYPE with an
owned serde body: generated code that could not compile. Found wider than filed — C had the
identical hole (C++ does not: it delegates serde across the FFI). Now rejected at config time by
`ensure_owned_storage_for_payload()` at all six entry points, with a diagnostic naming the field;
the dead `is_phase1_supported()` (no callers, and its claim had become false) is replaced by the
real support matrix. 7 new toolchain-free tests. Support itself deferred to #344.
See `archived/0343-*`. (2026-07-28)

Recently resolved: **#336** — post-RFC-0060 bootstrap drift: `scripts/bootstrap.sh` targeted the
RETIRED `ros-launch-manifest` path and guarded on a file that can never appear, so it silently did
nothing and a fresh clone could not build the CLI. Fixed at all six surfaces (bootstrap, 12 doc
copies, AGENTS.md's wrong "NOT --recursive", `just doctor` now checking `nros-launch-resolve`, the
book's 0285 PATH footgun, the CLI crate map) and gated by
`scripts/check-retired-submodule-refs.sh` in `check-fast` — one grep that would have caught all 21
sites, and the `.github` half of #337 too. See `archived/0336-*`. (2026-07-28)

Recently resolved: **#267** — Cyclone descriptor mis-walked depth-2 nested types
(`Control`/`PoseStamped`); corrected descriptor in `0a8f30ccb` — `archived/0267-*`.

Recently resolved: **#244** — platform-surface asymmetries recorded as decisions. Serial/IVC (+
PlatformLibc) are a deliberate Rust-only carve-out (post-RFC-0054 the C headers are SSoT; a
primitive joins the C ABI only when a C consumer needs it, authored — never hand-mirrored). zpico's
`smoltcp_set_clock_ms` externally-fed tick is required today; unifying it onto the SSoT clock is a
phase-230 item. Documented in platform-implementation-notes.md. See `0244-*`. (2026-07-28)

Recently resolved: **#284** — `NROS_CYCLONEDDS_MAX_TYPES` (the cyclone type-registry size) is now
DERIVED from the SystemModel + auto-emitted into `.cargo/config.toml [env]` by `codegen-system`, so
an image can't `RegistryFull` at runtime. Distinct-type count mirrors the backend's expansion
(msg=1/srv=2/action=8+3 shared), rounded up to a power of two; a user-pinned-too-small value is
failed loud instead. Verified cargo `[env] force` reaches the dep's `option_env!`. See `0284-*`.
(2026-07-28)

Recently resolved: **#270** — `nros-rmw-zenoh` forced `zpico-sys` default features
(`platform-aliases` + `link-ip`) transitively, so `platform-orin-spe` couldn't drop the alias TU →
double-defined `z_*` clock symbols vs orin-spe's native `system.c`. Fixed: `zpico-sys` dep is now
`default-features = false`; the two features are re-supplied via nros-rmw-zenoh `default` + each
non-orin-spe `platform-*`, so orin-spe forwards neither. Byte-identical for every other target
(verified via `cargo tree -e features`). See `0270-*`. (2026-07-28)

Recently resolved: **#242** — RMW parity gaps (publisher GID / message-info) resolved as a
documented carve-out: both already exist in nano-ros's own shape. Message-info is surfaced via the
`message_info()` subscription builder (Rust + C); only the `take_with_info` VTABLE slot is carved
(info rides a side-channel, keeping `try_recv_raw` lean). Publisher GID rides the zenoh wire
attachment → `MessageInfo.publisher_gid` (per-message attribution); only the standalone
`rmw_get_gid_for_publisher` query is carved (no consumer). Corrected the stale carve-out text in
`book/src/design/rmw-vs-upstream.md`. No code change. (2026-07-27)

Recently resolved: **#292** — nano-ros zpico ACTION SERVER now interops with a stock jazzy
`rmw_zenoh_cpp` client. Two bugs: entity liveliness tokens shared a hardcoded id `0/11` (five
action entities collided → action never assembled), and send_goal/get_result advertised the action
hash not their SERVICE hash (client query keyexpr missed). Fixed: per-session entity-id counter +
codegen-emitted `RosAction::{SEND_GOAL,GET_RESULT}_SERVICE_HASH`. Zenoh lane now 6/6; cyclone/xrce
non-regressing. See `0292-*`. (2026-07-27)

Recently resolved: **#291** — zenoh interop with a stock jazzy peer. The version gap (zpico 1.7.2
vs jazzy rmw_zenoh 1.11.2) was a RED HERRING — the zenoh wire is proto `0x09` on both sides (frozen
at 1.0), and a live handshake + delivery works. The real blocker was the RIHS01 keyexpr type-hash
tail: interop fixtures were built `ros-humble` (placeholder). Fixed by selecting the ROS edition on
the examples like the RMW (`ros-<edition>` passthrough feature) + the `ros_editions_zenoh` lane
(jazzy 5/6; ROS→nano action server is #0292). No zenoh version bump. See `0291-*` (kept in place,
reframed). (2026-07-27)

Recently resolved: **#269** — the freertos aggregate-declare wall was NOT the platform layer:
nros-rmw-cffi's `static_subscriber_storage` was hardcoded to 4 slots, so the 5th
`create_subscription` returned BAD_ALLOC and the executor masked it as
`SubscriberCreationFailed`. Pool now sized by `NROS_RMW_SUBSCRIBER_SLOTS` (default 8); the
executor preserves transport errors — `archived/0269-*`.

Recently resolved: **#274** — model-arm entry consumer walls: spin=forever arm,
`[param_services]` node identity, model-arm `system.toml` read (`9fc0fba25`) — `archived/0274-*`.

Recently resolved: **#283** — FreeRTOS liveliness on by default; the graph is fully visible
(`54175c040`) — `archived/0283-*`.

Recently resolved: **#253 + #255 + #258** — phase-306 interface-codegen correctness: per-package
FFI crates (superset archives retired, any combination links), launch/model remaps routed to the
wire with ROS 2 ~/relative expansion (runtime-proven, ws-remap-rust), and big-pkg cyclone deps
compile (derived-srv filter + IDL_DEPENDS + reserved-word escapes) — `archived/0253-*`,
`archived/0255-*`, `archived/0258-*`. (2026-07-26)

Recently resolved: **#261–#265** — phase-302 tier-knob honesty: posix caps truthed
(edf/reservation false, affinity true via the 296-W5.13 consumer), zephyr stack + posix
stack/advisory fail-loud, nuttx Rust tier priority adopted (marker e2e), sched_class
bake-rejected, tierless-target multi-tier diagnostic — `archived/0261-*`…`0265-*`. (2026-07-25)

Recently resolved: **#266** — time-slicing knob: added `time_slice_us` per-platform sub-table field
+ the ThreadX consumer (`nros_threadx_create_task` was hardwiring `TX_NO_TIME_SLICE`; now takes a
µs→ticks slice + boot `tx_thread_time_slice_change`, marker + e2e). Bake-validated ThreadX-only
(other RTOSes' time-slicing is a global config, rejected loudly). See `archived/0266-*`.
(2026-07-25)

**#260** — the RFC-0052 Native sched dims are e2e-verified only on the FALLBACK arm: every realtime
fixture is uniprocessor, so the SMP core-pin ACCEPT path (`k_thread_cpu_pin` /
`pthread_setaffinity_np` / `vTaskCoreAffinitySet`, `#ifdef CONFIG_SMP`) is compile-verified only.
Needs ONE SMP fixture (a separate zephyr native_sim SMP variant, not the shared image) to flip a
two-mode core-pin e2e to accept. See `0260-*`. (phase-296 W5.11 2026-07-24)

**#259** — derived scheduling is quantitatively INERT: the model carries no per-callback WCET
(`MapperPath.exec_ms` is `None` everywhere), so the budget dim short-circuits, blocking (`B_i`)
can never be numeric, and the feasibility check assumes `B_i = 0` — **unsound whenever callbacks
share a resource**. The originally-filed symptom (`placement`/`non_preempt_scope` hardcoded
`NotRequested`) follows from it: both are MECHANISMS, not requirements, so no contract fact
implies them. Rewritten 2026-07-26 with the design review — contention is already declared via
MutuallyExclusive callback groups (no new vocabulary for the intra-node case), a `holds:` contract
field is rejected (locking is implementation, not interface), and `criticality → core pin` is
rejected outright (unfalsifiable adjective; would look like a partitioning claim nothing backs).
Prereqs: WCET fact, board peripheral registry, `SchedCaps` core count. See `0259-*`.
(phase-296 W5.11 2026-07-24; reframed 2026-07-26)

(#286 resolved 2026-07-26 — see `archived/0286-*`: the blocker was NOT the missing runner but
`[unstable] build-std`, which is not target-scoped and rebuilds std against nuttx's patched libc
even under `--target <host>`. `probe_blocker()` now degrades such components to the documented
sidecar-less path; nuttx build-fixtures rc=2 → rc=0, qemu still probing.)

(#285 resolved 2026-07-27 — see `archived/0285-*`: ships `nros-launch-resolve`, a dedicated
distinctly-named helper built from a pinned play_launch submodule and invoked by ABSOLUTE PATH,
never $PATH. Kills the version skew and both directions of the name collision — we can't be
shadowed by the unrelated ROS 2 `play_launch`, and we never shadow it either.)

(#311 resolved — no feature SSoT across languages; one `nros_feature_set()` now serves nros-c, nros-cpp and the umbrella, and 50 Rust nodes stopped naming a ROS edition. See `archived/0311-*`.)

**#313** (resolved) — `nros_cpp_node_t.node_id` used `0` as BOTH "no node" and the FIRST node's id
(`NodeId` is an index into a table that starts empty), so a single-node C/C++ entry read as node-less
at eight call sites. Phase 268 fixed the adjacent half and left `0` overloaded. Now stored biased by
one. Found while investigating #312 — real, but NOT its cause. See `0313-*`. (2026-07-28)

Recently resolved: **#351** — build-stage fixture gates answered PRESENCE, not truth, which is how
#350 hid for three days. Fixed by phase-319: the suite stamp is cleared before the attempt it
certifies; the lane's 26 fixtures moved from six hardcoded shell arrays into `fixtures.toml`
(AGENTS.md:79 compliance, which is what let the staleness probe see them at all); and each row now
carries an `.inputsig` compared against current sources plus a `.build-failed` marker the resolver
treats as a hard error in EVERY tier. Acceptance: breaking a fixture now turns the LIGHT tier red —
the scenario #350 failed. See `0351-*`. (2026-07-30)

Recently resolved: **#350** — `NanoRosNodeRegister.cmake` never included the module defining
`nano_ros_auto_add_library`, so every DIRECT-include consumer failed at configure time with `Unknown
CMake command` — and `compile-check-fixtures.sh` (a `build-test-fixtures` prerequisite) had been
exiting 1 wholesale. The RFC-0057 split put the two verbs in two modules; `find_package(nano_ros)`
users get both, direct-include users (the build-stage fixtures, deliberately — issue 0041) got half.
Four fixtures had the shape, not one. Fixed in the MODULE, so a fifth cannot reproduce it.
See `0350-*`. (2026-07-29)

Recently resolved: **#342** — `orchestration_tiers_freertos` bypassed both sanctioned test seams: a
hand-rolled `qemu-system-arm` invocation (the interpreter already covered the board — it just missed
`-icount`, boot deadlines and log capture) and the only bare port literal among 14 `start_slirp` call
sites. 7447 was not merely unallocated: it sat inside ANOTHER platform's window. Now
`port_of(FreertosMps2, Rust, RealtimeTiers)`, with a guard asserting the firmware's BAKED locator
matches — that mirror cannot call the allocator, so it was free to rot. See `0342-*`. (2026-07-29)

Recently resolved: **#312** — a C/C++ listener received fine but was INVISIBLE to ROS 2 discovery
(`Subscription count: 0`). Root cause: the C examples pass `""` as the type hash, and that field is a
SEGMENT of the liveliness keyexpr — an empty segment yields a token `rmw_zenoh_cpp` does not count.
Delivery never uses the hash, so nothing failed. 8+ in-tree sources do this, including four
user-facing TEMPLATES. Fixed at the seam: nros-cpp normalizes an empty hash to the documented
`TypeHashNotSupported`. The QoS cells now assert both endpoints (serialized — discovery is a shared
resource). See `0312-*`. (2026-07-28)

(#320 resolved — committed `config/*_model.yaml` baked absolute host paths, so a model resolved on
exactly one checkout and `main_macro`'s `meta.inputs` `system.toml` lookup silently fell back to the
per-target leak elsewhere. Fixed in four steps. Step 3: content-addressed staleness in `cmd/ws.rs`
(re-hash `meta.inputs` vs disk, absolute path = stale) so the legacy models — never mtime-stale —
self-heal. Step 4: `nros ws sync` regenerated 67 models portable (43 under workspaces/ + 24 standalone
examples outside the original count) + `check-no-absolute-model-paths` gate. Step 1: `--bringup-root`
in the resolver (ff90416) makes relativity structural, not grandparent-inferred — proven to turn 3
absolute leaks into 0; `ws sync` passes it. Step 2: dead `meta.record` retired from the schema
(8452532). Step 5 decided: keep the relative pointer. Left open by design: no `deny_unknown_fields`
(step 2's back-compat relies on it). See `archived/0320-*`. main 07650d0a1 + this. (2026-07-28)

Recently resolved: **#319** — `cyclonedds-ci` was red on main for two days, so `just ci` could not
pass. `dynamic_bridge_seq_nested` failed `-1002 UnsupportedFieldType`. The #267 fix is CORRECT; its
regression test hand-built the `kinds[]` table in the pre-#267 flat layout (top-level fields at
[0],[1], children appended, the two `Small` subtrees ALIASED), which the preorder walker cannot
express — `emit_nested_body` starts at `k.inner` but `kind_span` assumes `idx+1`. Table rewritten in
preorder; production code untouched; `m_size >= 48` passes. 16/16. Landed red with the fix it guards
and was never filed — `cyclonedds-ci` is in `just ci` but not `just check`. See `0319-*`. (2026-07-28)

**#315** — after #314 swept the abandoned per-RMW trees, the only `<rmw>/` paths left in
`examples/` are the checker's own carve-outs. RESOLVED IN DESIGN: two of the three are not RMW levels
at all. px4's `cpp/uorb` + `rust/xrce` encode WHERE THE CODE RUNS — in-firmware (shares PX4's uORB
bus, skips serialization, interop with stock PX4 apps) vs companion (ordinary nano-ros nodes whose RMW
is pinned by `uxrce_dds_client`) — so they become `cpp/firmware` + `rust/companion` and the checker's
px4 exemption deletes itself. `zephyr/{rust,cpp}/cyclonedds/talker-aemv8r` is a plain violation (a
BOARD variant) and flattens. Then `allowed_roots` is empty. Scoping also found there is no uORB
interop example to separate — `nros-register-check` is a link assertion, not a demo — and that a uORB
bridge can't be a POSIX binary. Sequenced in phase-316. See `0315-*`. (2026-07-28)

Recently resolved: **#314** — `just zephyr build-examples` could not have worked in months: all four of
its dependencies were broken. `build-c` had its loop deleted and reported `built successfully!` at rc=0
having compiled nothing; `build-cpp`/`build-xrce` had `for ... do` followed straight by `done` (bash
syntax errors). Found because a 2-second "green" receipt for #316 was implausible for six Zephyr images.
Restored to delegate to `build-one`: 6 / 6 / 12 ELFs where there were 0. `build-rust-examples` remains
broken for an unrelated reason (`zephyr-build` unresolvable). See `0314-*`. (2026-07-28)

(#316 resolved — compile-time pool knobs that silently did nothing, three ways. `nros_resolve_knobs()`
now resolves each knob once (environment over Kconfig, disagreement prints) and both the cargo env and
the zenoh C defines read `NROS_RESOLVED_*`, so Rust and C cannot disagree (issue 0135 class). The five
`CONFIG_NROS_XRCE_*` options export + forward under the `NROS_XRCE_*` names their reader wants, and the
forwarding list is generated from the resolved set — "resolved but not forwarded" is now
unrepresentable. Two Kconfig defaults realigned so the repair moves zero bytes (`listener` build
byte-identical). Stage 3 enumeration deferred (out of scope); the build-c/cpp/xrce stub found on the
way was already fixed by #314. See `archived/0316-*`. fc6414598. (2026-07-28, filed as #313 and
renumbered on push — origin already had two))

Recently resolved: **#337** — `pr-checks` was RED ON MAIN for 60+ consecutive runs (back to
2026-07-27T17:17), unnoticed because everyone validates with `just check` locally. Two causes, both in
the `check` job: (1) `check-example-fmt` called `cargo fmt`, which runs `cargo metadata`, which loads
the leaf `.cargo/config.toml`'s `include` of the GENERATED, gitignored `nros-patch.toml` — a fresh
checkout never has it. The gate sits in `check-fast`, whose contract forbids exactly that ("no cargo
tree/metadata"); now calls `rustfmt` directly, which needs no dep graph. Phase-315's generated facade
crates gave the same call a second failure mode locally. (2) 8 workflow references to the retired
`third-party/ros-launch-manifest` submodule (phase-312 fallout; the tree was swept, `.github/` was
not) — same drift class as #336, which owns the bootstrap/doc surface. See `0337-*`. (2026-07-28)

**#338** — `spin` means the OPPOSITE thing across surfaces: C++ `Executor::spin(duration_ms)`
is bounded with no no-arg overload, while C/Rust spin forever and rclcpp's `spin()` blocks
until shutdown (bounded is `spin_some`); and the C registration family is half-renamed from
rclc — seven `nros_executor_register_*` but `nros_executor_add_client`. See `0338-*`.

**#339** — `rclcpp_compat::spin_until_future_complete` ignores the future on the timeout path
(always burns the full timeout) and returns `void`, so the standard
`== FutureReturnCode::SUCCESS` idiom cannot be written; the correct loop already exists in the
branch above it. See `0339-*`.

**#340** — #226 recurrence: `ParameterServer` still keeps array storage in a header-side bump
arena and recovers capacity via `reinterpret_cast<const uint64_t*>(ptr)[-1]` — an undocumented
pointer-identity contract with the C server that becomes an out-of-bounds read the moment the
Rust side copies an array. See `0340-*`.

**#341** — RESOLVED (2fabffd33): the test-matrix SSoT diverged from the supported axes — **uORB
was unexpressible** (`Rmw` enum lacked it though ARCHITECTURE §2 claims it + a crate+example
exist) and the declared `(ZephyrNativeSim, Cpp, Cyclonedds, Qos)` cell was "covered" by a
**Rust/Zenoh** test. Defects 1+2 fixed (uORB expressible via `Rmw::Uorb`+`PlatformId::Px4`+carve-out;
zephyr QoS cell corrected). Defect 3 (bind Interop/Bridge cells to the tests that run them) spun
out to **#352**. See `archived/0341-*`. (2026-07-31)

**#352** — RESOLVED (phase-324): Interop/Bridge matrix cells were not bound to the tests that run
them — a cell's declared `(lang, rmw)` could silently disagree with the fixture its test builds
(the #341-defect-2 drift class), with no coordinate to gate them (ephemeral peers, nano sides off
the west-leaves / `build-e2e-fixtures` lanes, not `fixtures.toml`). Fixed by extracting them to
`interop::CELLS` (nano `Cell` + peer + dir + build + test), a `Binding` SSoT, and gates G1–G4 that
replace the blanket `Kind::Interop | Kind::Bridge` fixture-coverage exemption (flipping the zephyr
QoS cell to the defect-2 shape turns G4 RED); `ci_lane` pools both lists; each interop test carries
an `assert_test_bound` coordinate tripwire. Residual (full runtime drive-rewrite) needs CI infra —
deferred, noted in the issue. See `archived/0352-*`. (2026-07-31)

**#342** — `orchestration_tiers_freertos.rs` bypasses both harness seams: a hand-rolled
`qemu-system-arm` command with no bypass rationale (while the next test in the same file uses
the interpreter) and the only bare `start_slirp(7447)` among 14 call sites, inside the
allocator's own window. See `0342-*`.

**#336** — post-RFC-0060 bootstrap drift: `scripts/bootstrap.sh:189` inits the RETIRED
`ros-launch-manifest` submodule path, so a **fresh clone cannot build the CLI** (it only works
here because two retired worktrees remain on disk); 9 doc copies of the dead command,
`AGENTS.md:285` says "NOT `--recursive`" (now required), `just doctor` checks the wrong prereq,
and the book still teaches the 0285 PATH footgun. P1. See `0320-*`. (audit 2026-07-28)

Recently resolved: **#321** — `output_marker_gate` was RED on main: six inline marker literals in
the ros-editions family, now the `output::*` constants (the service one via `service_result_line(5)`,
so it still pins the value). The gate also restated the table it guards; `MARKERS` now REFERENCES
`output::*`, which closes the direction the issue did not name — a marker renamed in output.rs used
to leave the gate policing a string nothing emits, green forever. Adding `ACTION_GOAL_ACCEPTED_PREFIX`
kept the gate's strength (the existing constant was the longer full line). Mutation-checked; all four
excluded binaries compile. The LANE GAP is untouched and still open: the gate polices sources whose
binaries never run in `just ci`. See `0321-*`. (2026-07-28)

Recently resolved: **#322** — `accept_goal` replied `accepted=true` BEFORE recording the goal and
discarded the `push` result, so with `MAX_GOALS` (4) active a 5th goal was acknowledged and then
dropped — no execution, no result, client waits forever. Capacity is now decided before anything
reaches the wire (full table → honest `reject_goal`), and a failed `send_reply` rolls the record back
so a slot cannot leak. Item 2 (propagate `publish_status_array`) deliberately NOT done: past
`send_reply` the acceptance is irreversible and both C/C++ callers collapse Err to a generic error, so
propagating would report "accept failed" for a running goal — reasoning recorded in code and issue.
`MAX_GOALS` as a documented knob stays open. NO TEST covers the 5th goal (`actions.rs:163` — the
client fixture cannot hold multiple goals in flight); action e2e green. See `0322-*`. (2026-07-28)

**#323** — parameter wire values silently truncated: `from_rcl_value`/`to_rcl_value` discard every
capacity error and still report success; unknown `type_` → `NotSet`; hosted `unwrap_or_default()`
turns oversize values into `NotSet`/empty arrays. P1. See `0323-*`.

**#324** — `spin_once` discards `session.drive_io()` errors and NO session-health surface exists →
a dead session spins `Ok(())` forever (same for the C blocking spins). P1. See `0324-*`.

**#325** — tool-resolver residue after #219: `integrations/nano-ros/CMakeLists.txt:82` uses HINTS
(stale `~/.nros/bin` CLI shadows the in-tree one) and then fails SOFT; a 5th bespoke `nros`
resolver caches dead paths; three `idlc` resolvers invert their own documented precedence. P1.
See `0325-*`.

**#326** — Zephyr guards keyed on the possibly-unset `NANO_ROS_PLATFORM` at 5 more sites (#282
fixed one of six, and introduced a second idiom). Latent, not live. See `0326-*`.

(#327 RESOLVED — the ROS-edition axis sat outside the test matrix. Documented edition as a per-run
global on `nros-tests::matrix::Cell` (not a per-cell axis — the code already treats it so); promoted
jazzy to supported in ARCHITECTURE §2 and moved the humble/iron `rmw_zenoh_cpp` carve-out out of a
code comment into ARCHITECTURE §2 + examples/README.md; collapsed the five per-cell `ros_editions_*`
files into one `ros_editions_e2e.rs` rstest over (rmw × workload × direction) = 18 cells
(live-verified cyclone pub/sub vs ROS 2 jazzy). See `archived/0327-*`. 2fabffd33 + eb520c046.
(2026-07-30)

**#328** — harness gaps: ~30 fixture resolvers in `binaries/mod.rs` still existence-only (#222's
freshness fix never propagated), and all **24 `#[ignore]` tests are unreachable** — nothing passes
`--ignored`, including the only gate on heap/borrowed storage-mode codegen. See `0328-*`.

(#329 RESOLVED — C++ headers carried runtime policy + wire decoding (#226 class). One
`nros_cpp_spin_for` CFFI now single-sources the wall-clock budgeted spin — killing nros.hpp's latent
iteration-count bug (`spin(1000)` collapsed to ms) and deduping the duration + bounded copies (the
unbounded loop's platform `yield`/`ok()` honestly stays header-side). The 2-arg `init()` forwards raw
to the 3-arg so the ladder lives once. `GoalAccept::ffi_deserialize` forwards to a Rust
`nros_cpp_action_goal_accept_decode` that owns the 17-byte layout. See `archived/0329-*`. 1a64eeb45 +
1e13df52d + e51d216ba. (2026-07-28)

**#330** — backend facts in agnostic layers: the zenoh locator default is hardcoded in 4 places
(two RMW-blind), `BoardConfig::zenoh_locator` names a backend in a core public trait, and the
façade macro hardcodes two `register()` calls. #225 class. See `0330-*`.

**#331** — RMW ABI seams: `create_session` carries zenoh's `whatami` as an undocumented `uint8_t
mode` with no rmw.h counterpart; `set_custom_transport` bypasses the RFC-0054 generated type with
no layout assert. Further rmw.h gaps appended to #0242. See `0331-*`.

(#332 RESOLVED — freestanding contract asserted but not enforced. `bridge.hpp` `<string>`/`<vector>`
gated on `NROS_CPP_STD` + new `check-cpp-freestanding-includes` source gate (the `-ffreestanding`
probe runs vs host libstdc++ and can't see the 0112 class); `check.h` printf now gated on
`__STDC_HOSTED__`/`NROS_CHECK_STDIO`, freestanding = no-op; `nros_rmw_cffi_register_named` rejects an
incomplete vtable at registration (`NROS_RMW_RET_INVALID_ARGUMENT`) so a partial backend fails loud
early instead of panicking mid-spin. Optional-slot typed-error downgrade deferred by design (contract
= complete vtable). See `archived/0332-*`. c61abe897 + df66f7bff. (2026-07-28)

(#333 RESOLVED — `nros new` emitted projects that don't run: the board dep vanished into a TOML
comment for 4/8 platforms, and the template was the retired `no_mangle` stub. Defect 1: one validated
`platform_spec` SSoT (esp32 reconciled into the clap parser), real board dep for every platform.
Defect 2: `scaffold_rust` dispatches on `PlatformKind` — hosted native/posix emit a runnable
`fn main()` (compile-verified via `nros sync` + `cargo check`); baremetal/esp32 emit `nros::main!()`
Form-1 + a `nros::node!` lib.rs; the split-package (freertos/nuttx/threadx) and west (zephyr)
platforms `bail!` loud before writing rather than emit a broken project. No platform emits a
non-running project anymore. Single-package scaffolding for the deferred platforms is a future
enhancement, not a bug. See `archived/0333-*`. main 8f81d2873 + this. (2026-07-28)

**#334** — hardcoded build-host Zephyr-SDK path in the test harness (`zephyr.rs:417`), the last
tracked absolute-path leak outside the SystemModels of **#320**; also a second SSoT for both the SDK
version and the host tuple. Shared `git grep` gate proposed with #320. See `0334-*`.

(#335 RESOLVED — two copy-out examples carried framework gaps. The PX4-SITL weak
`nros_rmw_cffi_register` stub moved out of the example into the uORB backend (b47f3d481). The Rust
lifecycle example's five raw `extern "C"` callbacks are replaced by a safe `LifecycleCallbacks` trait
+ `Executor::register_lifecycle_node` over monomorphized generic trampolines (phase-317, alloc-free,
symmetric with the shipped rclcpp-shaped C++ `nros::LifecycleNode`); the raw-FFI exercise is now a
co-located unit test. See `archived/0335-*`. b47f3d481 + 6b4032395 + 9e9915155. (2026-07-28)

**#316** — compile-time pool knobs that silently do nothing, two ways. On Zephyr,
`set(ENV{...})` is unconditional, so 20 of 61 knobs have their environment value overwritten by the
Kconfig default while the other 41 pass through — opposite precedence, identical spelling, no
diagnostic. Six of `autoware_sentinel`'s tuned knobs are dead this way. Separately, five
`CONFIG_NROS_XRCE_*` options are exported as `XRCE_*` while the only reader wants `NROS_XRCE_*`.
Includes a full audit: five distinct sizing mechanisms, only two of which a build-script hook can
see. See `0316-*`. (2026-07-28, filed as #313 and renumbered on push — origin already had two)

(#317 resolved — `wake-latency-cortex-m3` bench resurrected: build rot fixed, redesigned as two images
(pub + sub so zenohd routes a real transport-arrival wake), async-wake fixed in the zpico shim (fire the
runtime wake-cb from the read-task arrival hook), CSV emit fixed; test takes its intended CYCCNT skip on
QEMU, real P99 on hardware. See `archived/0317-*`.) (phase-313, 2026-07-28, renumbered from a
duplicate #313)

Recently resolved: **#309** (whole matrix audited) — count-based proofs detect an
ABSENT configuration, never a WRONG one. `Proof::QosMatchedCount` is now `QosMatchedProfile`, which
asserts the per-endpoint ADVERTISED profile via `ros2 topic info --verbose`. Mutation-checked, and
the check demonstrated the thesis: flipping the C talker's declared durability, DELIVERY STILL
PASSED and only the new profile assertion caught it. Audit answer: C/C++/mixed were fine all along —
but nothing had established that. Second pass found `LoggingLines` in the same class (it grepped the
log MESSAGE only, so a bare `printf` satisfied it) — now requires the facade's `[INFO]` tag on the
same line. `LifecycleActive`, `CustomMsgFields`, `SafetyCrcCount`, `RemapWireName` are sound.
See `0309-*`. (2026-07-28)

(#310 resolved — every fmt recipe now uses PLAIN `cargo fmt` (never `--all`, which follows path-deps
into the vendored submodules). Closed the coverage gap: the in-tree `packages/cli` sub-workspace gets
a `format-cli` recipe + a `check-cli-fmt` gate (both plain, submodules untouched); documented the
`--all` hazard. See `archived/0310-*`.)

Recently resolved: **#308** — a model's `qos_overrides.*` configured QoS on a C++ image and silently
nothing on a C or Rust one. `emit_c` had no QoS code at all; the Rust path had no MECHANISM (overrides
lived on `NodeHandle`, while components install through the executor). `plan_from_model` also built
every node with an empty table, so even C++ got nothing on the model path. Both halves fixed;
`qos_override_e2e` now covers it. Filed retroactively — it shipped and was cross-language.
See `0308-*`. (2026-07-28)

Recently resolved: **#307** — model-path parameter resolution diverged from rcl in two ways: a param
file's sections merged in TEXTUAL order (so a `/**` block written last beat the node's own), and
`ParamValue` → String rendered `1.0` as `"1"`, which the runtime re-types as INTEGER. The specificity
rule HAD been implemented correctly — in the copy phase-296 retired; the copy that shipped never had
it. Fixed by single-sourcing both in the model crate. Filed retroactively. See `0307-*`. (2026-07-28)

Recently resolved: **#306** — a declarative Rust node's per-entity QoS was DROPPED: `EntityMetadata.qos`
existed, the declarative API populated it, and `ExecutorSink::create_entity` never read it — every
publisher/subscription ran `QosSettings::default()`. `ws-qos-rust` had demonstrated "the visible
behaviour" of a transient_local profile for three phases while its e2e asserted only a message COUNT,
which default-to-default delivery satisfies equally. Plan `qos_overrides` DID apply (they fold inside
the executor), so the model could set QoS the code could not. Fixed + covered by the new
`qos_override_e2e`. See `0306-*`. (2026-07-28)

Recently resolved: **#303** — QoS overrides: an unmodelled policy or a misspelled role/policy was
dropped by a `filter_map` in every language, with no diagnostic. Fixed by collapsing FOUR copies of
the runtime decoder into `nros_rmw::decode_qos_override*` and BOTH bake-time lowerings into
`nros_orchestration_ir::qos_override`, adding `deadline`/`lifespan`/`liveliness`/
`liveliness_lease_duration` (codes 4-7), and making every rejection a build error that names the
parameter and the accepted spellings. A stale test had documented the gap as intended behaviour
(`lifespan` was its example of an "unrecognised policy"). See `0303-*`. (2026-07-28)

(#302 resolved 2026-07-28 — see `archived/0302-*`: `emit_rust` now bakes params, remaps,
identity and QoS overrides like `nros::main!`, with a parity gate so the fifth feature cannot
drift the same way. Its only consumer test has been stale since phase-296.)

**#288** — self-contained standalone examples (node + entry in one crate) dep their board
crate, so they cannot be host-compiled and cannot be metadata-probed. Executor sizing falls
back to the SystemModel bound for them; issue 0257's boot failure stays reachable if a user
grows one as a template. See `0288-*`. (renumbered from a duplicate #286)


(#293 resolved 2026-07-27 — see `archived/0293-*`: `system.toml` had TWO parsers with different
schemas, so rlm silently dropped `launch = "…"` on deploy blocks and counted launch-scoped
blocks against every launch file; `<node machine=>` was ignored as a placement too. demo_bringup
could not be re-resolved at all. Fixed in rlm + play_launch. The SSoT question — one deploy
schema instead of two mirrors — is recorded there as follow-up.)

(#305 resolved 2026-07-28 — see `archived/0305-*`: one probe project per workspace instead of
N. 8.8 GB for 3.5 of 6 components became 2.9 GB for all 6.)

(#304 resolved 2026-07-28 — see `archived/0304-*`: nros-cpp has TWO build paths and the
`NROS_EXTRA_CPP_FEATURES` hook was only on the umbrella one; the probe takes the other. Probe now links, runs, and records:
all six C++ components produce sidecars.)

**#311** — no SSoT for the cargo feature list: THREE cmake assemblies (`nros-c`, `nros-cpp`,
`NanoRosRuntimeCrate`) plus every Rust leaf naming its own `ros-*`. Two of the three hardcode
`ros-humble`, so a non-humble build compiles the runtime as humble while codegen bakes other
type_hashes — a wire mismatch, not a build error. And because cargo features are additive and
`ros-{humble,iron,jazzy}` are `compile_error!`-exclusive, leaves naming an edition make
multi-edition impossible: the edition is an IMAGE-level choice, not a package one. Blocks
multi-edition + selectable capabilities (`param-services`, …). See `0311-*`.

**#287** — a host-only workspace member breaks `check-workspace-embedded` through cargo
feature unification, and the error names `nros-serdes` rather than the crate that caused it.
The `--exclude` list is manual and duplicated. See `0287-*`.

**#200** — fixture-build timing campaign blocked on a big-disk CI runner (phase-226 validation
residue). See `0200-*`.

(#270 resolved — `nros-rmw-zenoh` pulled `zpico-sys` DEFAULT features, double-defining clock symbols on orin-spe. See `archived/0270-*`.)

**#271** — Orin SPE BTCM footprint regressed ~+195 KB between `d9af52be` and `21a3a4248`; a
minimal `Executor::open`+spin image no longer fits 256 KB. See `0271-*`.

(#272 resolved — `nros sync` now INLINES the nros/nros-core/nros-serdes trio with absolute paths for
out-of-tree consumers (no fragile `include`); in-tree example leaves keep the committed relative
include. Failure class `no matching package named 'nros'` eliminated. See `archived/0272-*`.)

(#273 resolved — `nros-board.toml` `[board.entry]` signature `_start`→`main` (matches `board_mps2.c`
`Reset_Handler`); the scaffold stub fixed too; the off-lane wake-latency bench spun to `#0317`.
phase-313. See `archived/0273-*`.)

(#284 resolved — `NROS_CYCLONEDDS_MAX_TYPES` hidden compile-time knob, twin of the fixed `NROS_EXECUTOR_MAX_CBS`. See `archived/0284-*`.)

**#278** — no polling subscriber / blocking service futures: mrm_handler-class ports weaken to
cache-latest subs + send-and-poll service calls (semantic weakening of the safety path,
documented in-source). take()-style sub wanted — note the original "RMW already caches latest
per sub" premise is FALSE (corrected 2026-07-26; needs new retained storage). The bounded-wait
call already exists; its in-callback hazard split out as #290. See `0278-*`.

(#290 resolved 2026-07-26 — see `archived/0290-*`: nros-cpp blocking helpers had no
reentrancy guard, so calling one from inside a callback aliased `&mut Executor` silently;
C guarded it all along via `nros_executor_t.in_dispatch`. Fixed with a `DispatchGuard`.)

(#278 + archived #276, #277, #279-#281 from the simple-autoware-safety-island port
friction log, filed 2026-07-25 from docs/porting-notes.md 06/12/14 + the
same-day-fixed 10/16/18. #275 resolved by RFC-0057/phase-305 — see
`archived/0275-*`: L.4 retired, upstream namespaces port verbatim. #276 resolved
2026-07-26 — see `archived/0276-*`: `<param from=…>` parses, param-file values
project into the generated entry; seeding gated on `param_services`. #277 resolved
2026-07-26 — see `archived/0277-*`: phase-306's per-package FFI crates remove the
mixed-subset failure mode by construction (verified on a disjoint-subset workspace,
zero duplicate exports); no union closure needed, union-shim pkg obsolete.)

**#248** — Embassy board entry is a stub: every Board/EmbassyBoardEntry method `todo!()`, the C.3
dispatch body is a placeholder — images boot but callbacks never fire (RTIC twin is complete,
phase-289). Release decision: finish or de-advertise. See `0248-*`. (release-prep audit 2026-07-24)

(#244 resolved — platform ABI surface asymmetry: PlatformSerial/PlatformIvc had no C header mirror. See `archived/0244-*`.)

(#243 resolved — board-trait duplication ended: `board_init` deleted; two canonical board APIs
(`nros_platform::board` Rust + `<nros/board.h>` C), gated by `check-no-board-init`. phase-313.
See `archived/0243-*`.)

(#242 resolved — RMW parity gaps vs rmw.h: publisher GID + message-info out-param. See `archived/0242-*`.)

Recently resolved: **#240 + #241** — phase-301 landed the batched RMW shape alignment on the
RFC-0054 header SSoT (create_session/subscription/service/client terms, hints in options structs,
call_raw deleted, fallible QoS boundary lowering with the ceil/reject semantics) — `archived/0240-*`,
`archived/0241-*`. (2026-07-24)

Recently resolved: **#246** — realtime_tiers_e2e nuttx_arm_rust cell timed out on a fresh image:
TWO Rust-arm defects behind one timeout — (1) spawned tiers used the libstd default 2 MiB stack
(NuttX `pthread_create` ENOMEM → "failed to spawn tier"; fixed with explicit `.stack_size()` =
`stack_bytes`/64 KiB + a bounded retry for the transient under-load spawn flake), and (2) the
session-owning boot tier (`high`, prio 110) was budget-capped — `apply_tier_sched_policy` installs
the tier SchedContext as the executor DEFAULT, gating the shared-session flush (measured `ctrl=1`);
now the boot tier stays Fifo + skips kernel sporadic (non-owner tiers still realize the budget,
matching the C++ per-handle-bind behaviour). Cell green solo 6/6 — `archived/0246-*`. (riscv trio
lane-name mismatch left as a separate test-plumbing note in the archived issue.)

Recently resolved: **#236** — multihost `machine=` fully closed: play_launch carries `machine`→`deploy.host` (46.1) and rlm `6d64202` makes machine-only deploys UNPLACED (`Deploy.target: Option`); nano-ros slices treat unplaced as board-agnostic (host filter partitions), `zephyr_entry_robot1` migrated — `archived/0236-*`.

Recently resolved: **#245** — zephyr C/C++ multi-tier heap-corruption crash: executor storage was a
hardcoded 80 KiB while the real generated size grew to 81952 (32 bytes short) — the tier executor's
tail overwrote the next sys_heap chunk, subscriber-delivery-gated; fixed by `__has_include`ing the
generated `NROS_CPP_EXECUTOR_STORAGE_SIZE` (+ guarded-include on the freertos/nuttx mirrors —
NuttX-arm was 856 bytes from the same cliff); all three zephyr realtime cells green —
`archived/0245-*`.

Recently resolved: **#268** — freertos C lanes red = the sizes-header MIRROR
race recurring (0088/0114 class): 296-W3b.4 grew the Executor, the
include-path shadow of `nros_config_generated.h` stayed stale on incremental
fixture rebuilds → 336-byte `_opaque` placement overflow → register -1 +
session self-close on first TX (`declare -128`). Clean rebuild = 3/3 green.
Full-stack bisect → `63d271f43`; zenoh-pico/294-serialize/ports ruled out.
Class gap (mirror not in the incremental graph on this path) noted inside.
See `archived/0268-*`.

Recently resolved: **#239** — RMW ABI hand-mirror now field-parity gated
(`check-rmw-abi-mirror`: vtable 36 slots + 8 entity structs, ordered names;
fired on a LIVE Rust-only QoS `tx_express` split — C side couldn't request
express — header + profile macros fixed); the platform 3-way mirror was
already exhaustively gated by `check-platform-abi-mirror.sh` (the audit
judged only `c_stub_platform.rs` and missed it). See `archived/0239-*`.

Recently resolved: **#237** — `ws-safety-{c,cpp}` migrated to `MODEL` (4
entries + per-variant models) and validated through the fixture builder's
`-safety-*` rows (both native lanes green) — `archived/0237-*`. **#232** — FVP runtime gate landed (phase-298): ws-entry publishes on the model (board eth + heap + SLIRP IP), `west fvp` registered, `verify-fvp-runtime` maintainer verb + `fvp_runtime_ws.rs` (asserts `[ctrl] tick=`/`[telem] tick=`), false-green legacy talker tests deleted — `archived/0232-*`. **#238** — RMW event-kind ABI width bug fixed: `NrosRmwEventKind` `#[repr(u8)]`→`#[repr(C)]` to match the int-sized C `nros_rmw_event_kind_t` passed by value across the vtable; drift now gated both sides by compile-time layout assertions (Rust `abi_layout` const block + C `abi_layout_check.c`), both proven to fire on injected drift; also fixed two latent 1-byte `transmute(kind)` sites in the adapter (now an explicit `From`) — `archived/0238-*`. **#233** — the RMW runtime-coverage backlog is empty: every worth-implementing cyclone cell (native rust service+action, threadx-linux C service+action + C++ pubsub, threadx-riscv64 C++ pubsub) is now Runtime — `archived/0233-*`. **#235** — threadx-riscv64 C++ cyclone pubsub lane added (the fixture already existed with distinct per-node identity; only the two-QEMU consumer was missing) — `archived/0235-*`. **#231** — Zephyr multicast join fixed (fork 1d794c0a:
`struct ip_mreqn` + `-EALREADY`-is-success; Zephyr's handler rejects the
classic `ip_mreq` by optlen). Both joins clean on the FVP, closed loop at
~19 Hz, unicast-only fallback no longer engages. See `archived/0231-*`.

Recently resolved: **#234** — native rust cyclone action now delivers the order-10 Fibonacci
result. Two root causes: (1) `RosAction::register_protocol_types` registered the `action_msgs`
descriptors (CancelGoal_*, GoalStatusArray) via a `#[cfg(feature = "rmw-cyclonedds")]`-gated
named-backend call that compiled out of the example build → now routed through the generic
`nros_rmw::register_type_descriptor` seam, and CALLED from the `node.rs` typed action paths (they
never invoked it); (2) the Cyclone backend's `action_effective_base` / `action_topic_type`
doubled the per-channel wrapper infix (`Fibonacci_SendGoal_SendGoal_Request_`) when the typed path
passed the already-suffixed type → made idempotent. See `archived/0234-*`.

**#230** — SMP: spurious `ComponentNode failed at ? (code=0)` FATAL print on a
healthy boot (cross-CPU visibility race on the ok-flag; single-core never
prints it; the same race could mask a real failure). Found on the ASI FVP
SMP-4 validation. See `0230-smp-spurious-component-failure-print.md`.

**#229** — C `nros_ret_t` and C++ `nros::ErrorCode` disagree from -5 down
(`ALREADY_EXISTS` vs `Full`), and `Result` is built straight from C codes —
error misreporting class, found during the phase-292 ASI FVP bring-up. See
`0229-c-cpp-ret-code-enums-disagree.md`.

Recently resolved (see [`archived/`](archived/) for the full list): **#230** — the SMP
spurious "ComponentNode failed at ? (code=0)" boot line: the state was `ok_=true` (needs a
store; pre-ctor BSS-zero read as not-ok on a cross-core reader). Inverted to a zero-default
`has_error_` — healthy is the universally-visible zero state, failures release/acquire-published
(closes the miss-a-real-failure direction too); `__atomic` builtins, no `<atomic>`, zero
template changes. Native rclcpp poc healthy, 0 FATAL lines; live FVP-SMP gated on #232. **#229** — the
C `nros_ret_t`, C++ `nros::ErrorCode`, and `nros_cpp_ret_t` FFI codes now share ONE numbering
(C++ aligned to the canonical C side; a raw `-5` no longer misreads `ALREADY_EXISTS` as
`Full`); static_assert pin tables in result.hpp/parameter.hpp/node.hpp fail the build on any
re-divergence (proven live). **#228** — one C
serialize convention (phase-294): services + actions joined the message shape (0/-1 +
size_t* out-param, 5 emitters); 28 consumers migrated; all five platforms' service/action
lanes 37/37 on regenerated typesupport. **#221** — stale-docs
premise: stm32f4 rtic/embassy `init_hardware` has been REAL since phase-289 (`c2227f527` — QEMU
RTIC pubsub/service/action lanes all green on the shared entry scaffold); no `todo!()` panic
exists. The 8 example "Skeleton status" headers + the board doc now state the post-289 truth
with honest caveats (on-hardware bench run still pending; embassy has no e2e lane). Action pair
build-proven. **#227** — explicit
domain 0 is reachable from C/C++: `NROS_DOMAIN_ID_EXPLICIT_ZERO` (255) /
`nros::kDomainIdExplicitZero` maps to a baked `Some(0)` through the one resolver (0 stays the
unset sentinel per the #206 model-A decision; hosted env still overrides); u8→u32 domain type
unification recorded for the next ABI window. **#226** — the C++
ParameterServer sequence engine is gone: bytes stay in the inline pool (the stable owner the
borrow-FFI requires, capacity-header-prefixed), records moved into the C/Rust server via the
existing array FFI — no parallel name table, and sequence params are now visible to the param
services. **#222** — the four
rtos fixture resolvers now hard-fail STALE instead of running museum binaries (the #215 trap
closed on freertos/nuttx/threadx×2); found + fixed the deeper flaw en route: regenerated-in-place
cbindgen headers (nros_generated.h / nros_cpp_ffi.h / zpico.h) must be EXCLUDED from dep-graph
freshness — their mtimes are build side-effects that ping-pong across feature-variant families
(false-stale treadmill); zpico_drift_gate's compile-in-test is now a documented sanctioned
exception. **#223** — action
goal/cancel/result parsers propagate CDR read errors (truncated frame ≠ "goal rejected"); unit
tests added. **#224** — one shared SERVER_DISCOVERY_PROBE_TIMEOUT_MS. **#225** —
`cyclonedds_register` → `rmw_type_registry`, cfg → `rmw_needs_type_descriptors`; backend names
survive only in prose + the backend's own C symbols. **#220** — last phantom `esp32` board ids
fixed; the "missing verbs" half was a haiku-lane false positive (`nros release` is hidden by
design). **#219** — one shared
`nros_resolve_cli` (NanoRosCodegenCore) replaces the four divergent cmake resolvers; PATH-wired
in-tree CLI always beats the provisioned store (PATHS never HINTS), `$NROS_CLI` overrides,
stale caches re-detect. Precedence proven by direct tests. **#217** — RETIRED:
`build-fvp-aemv8r`/`run-fvp-aemv8r` deleted (unbuildable since phase-221 dropped the west src
arg; purpose covered by the cyclonedds-rust lane, which matches the ASI consumer's RMW);
phase-292 W1.a's workspace-Entry FVP lane is the modern replacement. **#218** — the
std_msgs raw-CDR components are gone: 15 files across workspaces/c, ws-qos-*, and 5 templates
migrated onto generated bindings (C `_serialize`/`_deserialize`, cpp typed `bind_subscription`
member callbacks); all six workspaces build, lanes green incl. the NuttX cross-process entry.
Raw-path demos (zephyr srv/action CDR, component-poc, custom-platform) deliberately kept. **#212** — workspace
custom msgs get GENERATED C/C++ typesupport (phase-293): resolve-deps gained the
`NROS_INTERFACE_SEARCH_PATH` workspace layer + cmake threads it to the CLI child; all three
ws-custom-msg workspaces rewritten off hand-rolled CDR onto generated bindings (the feared cpp
double-builtin_interfaces glue edge is dead); custom-msg lanes 10/10. Residual std_msgs raw
components → #218. **#216** — the FVP
AEMv8-R cyclone RUST lane is green again; three rot layers: the `examples/zephyr` nightly pin
never listed `aarch64-unknown-none` in `targets` (E0463 — a hand-added target on an older
nightly was lost in a pin bump; now declarative), missing inert `rmw-zenoh`/`rmw-xrce` feature
rows (0163-era `zephyr_component_main!` check-cfg), and a newer-nightly doc-list lint. New
`just zephyr build-fvp-all` sweep verb guards the rot. **#204** — the
clean-system bootstrap probe exists: `just probe bootstrap` runs the book's `probe=NN`-tagged
setup blocks verbatim (extracted, never hand-mirrored) on a pristine `ubuntu:24.04` container
and asserts the first-node readiness signal; nightly `bootstrap-probe` job on the 07:00 cron.
Its first run caught four real fresh-host regressions (cargo PATH, lost bundled
std_msgs/builtin_interfaces, zenohd off PATH, missing `nros sync` step in the book) — all
fixed. **#211** — phase-291:
`nros-zephyr-build` owns the canonical zephyr-leaf Kconfig→rustc-env bake (zero-dep; upstream
`zephyr-build` is west-path-only so its call stays in the leaf); all **14** leaves collapsed to a
4-line `build.rs` (the W4 grep-gate `example_shape::zephyr_leaf_buildrs_uses_shared_bake`
immediately found the 14th — `cyclonedds/talker-aemv8r`, which had NO bake: the 0161
silent-domain-0 class); ws entries gained the drifted-away XRCE synthesis; 67-test zephyr sweep
green on rebuilt fixtures. **#215** — RETRACTED as
filed: the "broken fresh threadx-linux cyclone talker" was an orphaned pre-287-W6 museum binary
(`threadx_c_talker`; W6 renamed the target `c_talker` and the test kept the old hardcoded path —
`exists()` happily executed the orphan). Fresh `c_talker` publishes fine; one-line test fix.
Lesson: target renames must grep tests for hardcoded binary paths, and gdb `finish` retvals are
garbage under the tx-linux signal scheduler (use `NROS_RMW_TRACE_OPEN=1`). **#206** — the env
overlay (`NROS_LOCATOR`/`ROS_DOMAIN_ID`/`NROS_NODE_NAME`) is now applied in ONE place —
`ExecutorConfig::try_resolve` (RFC-0045 model A) — for Rust, C (which gains it), and C++
(whose duplicated header blocks are deleted); malformed or >`DOMAIN_ID_MAX`(232)
`ROS_DOMAIN_ID` is an init ERROR everywhere, never a silent domain-0. Model-A decision:
hosted env overrides explicit init args, per ROS convention; domain 0 stays the unset
sentinel — supersedes the same-day helper-based fix (a0061e36e), whose explicit-args-win
semantics contradicted the maintainer's model-A decision. **#214** — the rust
cyclone riscv64 lane is REAL now: new `test_threadx_riscv64_cyclonedds_two_qemu_rust_pubsub`
(the resolver's first consumer) passes ~7.7 s. The actual wire identity on this path is the
cmake-generated `NROS_APP_CONFIG` (10.0.2.x, applied by startup.c pre-kernel) — both images
booted .40/:56; the second-node examples now set `NROS_APP_NET_{IP,MAC}_LAST` cache vars in the
CMakeLists preamble, and `Config::default()` bakes `NROS_DOMAIN_ID` (corrosion env via the #205
seam) so the images join the fixture's domain 62. The cyclone boot path also prints an
`[app] MAC/IP/domain` banner now — the silent identity is what hid the collapse. **#205** — all four
riscv64-threadx rust boilerplate classes retired: the hand cyclone-descriptor shims (redundant
since #195's `.init_array` walk), the `app_main` FFI trampolines (now the board's
`cyclonedds_app_main!` macro), the per-example critical-section dep+anchor (moved into the
board crate), and the CMake tail (new `nros_threadx_rv64_rust_cyclone_app()` board-overlay
seam; `cyclonedds_app.c` deleted from all 6). Cyclone + zenoh builds green; the macro image
boots and publishes. Lane-wiring residuals → #214. **#213** — batched
DECLARATIONS outran the action server's readiness banner (router log: goal query "no matching
queryables" 152 ms before the queryable declare landed); fork fix: declares always bypass the tx
batch (control-plane, same as requests/replies). With it, the phase-282/290 zephyr flip is LIVE
(batch+split default on): pristine zephyr suite 46/46. Manual-repro trap re-learned: unseeded
native_sim pairs share a ZID (#157) — the harness seeds, hand runs must too. **#207** — zpico
size-probe failure now HARD-FAILS on cross targets (panic naming the corrupt-ABI consequence)
instead of silently shipping guessed socket/endpoint sizes; host-native keeps the warned
fallback. **#208** — setup.bash/
setup.fish are now deprecated shims sourcing activate.sh/activate.fish (legacy NROS_ROOT kept);
justfile hints + 3 book pages repointed. **#209** — book's phantom `esp32` board id →
`qemu-esp32-baremetal`; `nros init` got its missing CLI-reference section. **#210** —
packages/cli/CLAUDE.md rewritten for the in-tree sub-workspace (was the retired
colcon-cargo-ros2 guide). **#203** — the
mixed-workspace cpp FFI compile failure no longer reproduces (fixed en route by the 263-A4
idempotency + 269 header-mirror repairs; the "over-generation" was a misread —
`example_interfaces` really depends on `action_msgs`). Landed the blocked demo: the mixed
service pair is now genuinely cross-LANGUAGE (C server + C++ client), e2e-green; the cpp pkg in
the mixed generation is the standing regression site. **#201** — option 2
(real element lifetime): `HeapSequence` dtor/move-assign/`clear` run element destructors
(pseudo-destructor loop, zero-cost for trivial `T`); `reserve` byte-relocates (documented
trivially-relocatable element contract); placement-new `push_back` + new `emplace_back()` for
owning elements. Generated FFI gained a recursive `teardown_*_fields` (the `_fini` analog) +
zero-inited nested element buffers, so deserializer error paths tear down partial elements.
Runtime lifetime probe (counting allocator) runs in `check-cpp`. **#202** — re-triaged:
15/17 red tests exercised the phase-172 "generated standalone system package" pipeline whose
verbs were removed in phase-222 — retired the dead path (−9,346 lines; live bridge rendering
moved to `orchestration/bridge_gen.rs`), salvaged the live plan/check/metadata coverage as
`plan_pipeline_e2e.rs` (fixing 4 live bugs that rotted while nothing ran the suite: probe
`[workspace]` capture, retired `record_component_metadata` name, pre-212.K fixture `RosAction`,
pre-M-F.17 record pair), and wired `just check-cli-tests` into `check-build` (~870 CLI tests
now run on every `just check`; a cwd race in `phase_212_f_bringup` surfaced immediately,
serialized). **#199** — the
`build-riscv-c` ffi-link red (undefined `std_msgs_msg_string_*`) was the riscv board
cmake overlay being a stale pre-phase-281 mirror of the arm one: the phase-281
`INTERFACE_SOURCES` walk (generated C serdes `.c` → cc-rs → trailing `app_iface`
archive), the phase-263 C2b component-source walk (+ `SOURCE_PKGS`), and the 0149
component-lib descent were never ported. Ported verbatim; lane green, fixed talker
boot-verified. Lesson: a link_app change in the arm nuttx overlay must port to the
riscv twin in the same commit. **#178** — the RTIC
lanes deliver (phase-289): six stacked layers — open-in-`#[init]` [pre-fixed], no `wfi` yield, no
armed IRQ through RTIC's vector table (fixed: CMSDK TIMER0 + macro-emitted priority-2 `binds`
tick + `on_interrupts_live` → `enable_wfi_idle`), `register_dispatch`-only wiring that never ran
`Node::register` (no entities → nothing published; now the owned-spin `register()` seam inside
`__nros_run`), no `nros_log::init` (#191 class), and service/action pairs baked onto four
DIFFERENT router ports none matching the harness table + the service test grepping the retired
4-call banner (#157 class). All four rtic lanes green — first proven green post-`Executor<'s>`.
**#165** — phase-285
W3–W6 landed the full riscv-nuttx Model-1 runtime: `QemuRvVirt::run_tiers` +
`entry_net_init` eth0 push, the `nuttx-riscv` board key, a `ws-realtime-rust`
riscv entry sharing the arm 2-tier plan, and `realtime_tiers_riscv_nuttx_e2e`
GREEN (~12 s, #158 counter proof). Two defconfig fixes en route:
`CONFIG_SYSTEM_TIME64=y` (Rust libc fork `time_t = i64` vs 32-bit kernel default →
std `invalid timestamp` panic in session bring-up) and dropping
`CONFIG_NETUTILS_TELNETD` (empty-builtins stub → `strlcat(NULL)` boot fault).
riscv-nuttx stays an off-matrix board (documented in `exec_model_matrix.rs`) while
#199 keeps the C lane red. **#198** — wontfix
(option B): documented source consumption IS the ESP-IDF contract (clone + bootstrap +
path dependency, micro-ROS precedent); registry publish rejected on verified facts — the shell
pack is runtime-less, a whole-tree pack can't be turnkey (Rust toolchain + nros CLI required,
per-consumer codegen), and a manifest git: dep would recurse all 23 submodules. Revisit on
Espressif submodule filtering or a discoverability mandate. **#183** — declarative
ws-bridge 0-sample lanes were a fixture type mismatch, not a bridge bug: both ws-bridge demos
forward `std_msgs/Int32` on `/chatter` while the shared talker/listener fixtures defaulted to
String post-277-W4.b. Cyclone lane got the `NROS_PUB_TYPE`/`NROS_SUB_TYPE=int32` alignment on
2026-07-13 (verify was blocked on #193); the xrce test got the same alignment 2026-07-15. All
three lanes PASS on fresh fixtures (6/6 across the declarative + imperative bridge families).
**#195** — the
threadx-riscv64 cyclone two-qemu zero-delivery: the descriptor-registration ctors never RAN —
the flat bare-metal image walks no `.init_array` (orphan section, no bounds), so every
reader/writer create failed -1. Fixed with lds `.init_array` bounds + a board ctor walk, atop a
buildability stack (recipe now passes `-DNANO_ROS_PLATFORM=threadx_riscv64` post-287-W6; the
cyclone `NROS_PLATFORM_THREADX` gate matches `^threadx`; the toolchain resolves a
rv64gc libstdc++ + `-lnosys`). Pubsub passes 2/2 in ~6 s. **#190** — the esp32
"session-init memory corruption" (incl. the old 0xffffffff residual) was a STACK OVERFLOW:
`.stack` on esp32-c3 is the linker leftover after `.bss`, so the 96 KB heap fix left 18 KB of
stack for a ≈98 KB-deep zenoh+smoltcp path — frames wrote into `.bss`, masquerading as heap/
cookie corruption (allocator instrumented: zero foreign frees). Heap 48 KB = executor arena fits
AND ~67 KB stack; plus the ws-entry test forgot `NROS_SUB_TYPE=int32` (Entry publishes Int32,
type is in the keyexpr — String sub matches nothing). esp32 suite 8/8, baremetal 11/11. **#171** — the
no-external-distribution umbrella closes: D1/D2 source-distribution bootstrap (phase-288), the
RFC-0048 ament CMake shape + W9 Rust consumption (phase-287, complete), false claims all
truth-fixed; the single live remainder (ESP-IDF registry execution) is #198. **#194** — the
threadx-linux rust rtos-e2e zero-delivery was three stacked defects, none in the runtime:
museum pre-212.L role binaries satisfied a retired builder path (the freertos #181 entry-image
repair was never applied here), the board crate lacked the #131 `rmw-zenoh` forwarding so every
entry image booted with NoBackend (`Executor::open` ConnectionFailed, zero wire I/O), and the
rust entry `main` lost `startup.c`'s `setvbuf` so a piped harness never saw the readiness
banner. Entry builders + markers + per-variant baked ports + board feature + stdout
line-buffering landed; pubsub/service/action all pass. **#191** — the freertos
rust `*-entry` lane delivered all along: the board installed no `log::Log` backend, so the
components' `log::info!` markers (`Publishing:` / `I heard:`) were dropped and the
marker-counting harness reported 0. `install_uart_logger` (threadx shape) ported into
`nros-board-freertos` entry; all three rust e2e lanes green 3/3 (full freertos matrix 9/9).
Bare `nros::main!()` is Form-1 self-bringup (entry lib re-exports `register`) — the empty
step-2 launch placeholders are dead files, not the cause. **#192** — the FVP
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
race](archived/0301-run-tiers-spawned-tier-declare-race.md): the chained-spawn fix
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
Resolved at [phase-265](../roadmap/archived/phase-265-ws-sync-config-patch-toml-edit.md) W4 — `nros sync`
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
See `archived/0294-nuttx-typed-carrier-link-drops-platform-port.md`. (Renumbered
from a duplicate id 48; the FreeRTOS-slirp issue keeps 0048.)

Recently resolved (Phase 240.5): **#47** — C/C++ action client now callback-based
(`nros::bind_action_client` = `set_callbacks` + a poll-timer pump per RFC-0041);
NuttX cpp+C action E2E green in QEMU. See `archived/0047-*`.

**#44** — esp-idf `platform.c` compile failed: esp-idf riscv `FreeRTOSConfig_arch.h`
uses linker symbols `_heap_start`/`_heap_end` (`&_heap_end - &_heap_start`) this TU
never declared. Fixed by declaring them `extern int` (matching esp-idf), gated to
`ESP_PLATFORM`, before `<FreeRTOS.h>`. Verified: esp32c3 `platform.c.obj` compiles.
See `archived/0044-*`.

**#0282** — resolved, see `archived/0282-*`.
