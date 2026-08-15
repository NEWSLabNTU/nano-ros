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
   resolution summary). Do the move in the SAME commit as the status flip —
   skipping it is how 39 resolved issues accumulated in `docs/issues/` by
   2026-08-02, with the "Open issues" list below carrying 86 rows for work
   that was already done.
3. **"Open issues" below lists exactly the files in `docs/issues/`** — one row
   per open issue, no more. A resolved issue keeps a row only under "Recently
   resolved", and only for the current cycle; after that `archived/` is the
   sole record. Verify with: `ls docs/issues/0*.md` versus the `**#NNN**` rows.
4. **Numbering** = the next integer after the highest existing id.
   **Slug** = a kebab-case form of the title; the filename id is the
   zero-padded 4-digit issue number.

## Issue vs RFC vs phase doc

- **Issue** (`docs/issues/`) = a bug, limitation, or tech-debt item.
- **RFC** (`docs/design/NNNN-*.md`) = a design decision.
- **Roadmap phase** (`docs/roadmap/`) = an implementation plan.

Issues cross-link to the RFCs and phases that inform or resolve them via the
`related:` frontmatter field.

## Open issues

Recently resolved (2026-08-16): **#620** — `NROS_PLATFORM_TASK_STORAGE_SIZE` was 256 B, sized from a 32-bit
port ("~232 B on 32-bit"), while ThreadX-Linux is a HOSTED port whose `TX_THREAD` measures 352 B — so the
`_Static_assert` fired and took out the threadx-linux family. Fixed upstream CONCURRENTLY by `199c8b0d3`
(phase-364 W2/W3), which raised the shared bound to 512; `lane=tier2` now reports `== threadx_linux == OK`
with zero assertions. Filed and resolved within the hour, so recorded rather than deleted: the assert is
phase-360 W5's replacement for zpico-sys's hand-computed "≈ with 2× margin" table (#0570), this was its
FIRST firing, and it fired on a real overflow — a silent overrun turned into a compile error naming the port.
See `archived/0620-*`. (2026-08-16)

Recently resolved (2026-08-16): **#519** the plan reported a 500 µs timer as `0ms`. The render was
ALREADY fixed in the tree (planner emits `period_us`; `explain.rs` prefers it and falls back to widened
`period_ms`) — what was missing is a test, so the exact defect could return silently one `unwrap_or_else`
away. Three added, and proven by sabotage: restoring the truncation fails the 0519 case and ONLY it.
STILL OPEN and now unowned: `SchedContext.period_ms`/`budget_ms`/`deadline_ms` carry the same truncation;
this issue deferred that to #505, which resolved without moving them. Carried to phase-357 W1, where the
unit for declared timing is settled once rather than per-field. See `archived/0519-*`.

**#619** (build/api, open 2026-08-16) — `cargo test -p nros-c` cannot LINK: `nros-log`'s `PlatformSink` calls
`nros_platform_log_write`/`_flush`, supplied by a platform C port that no test binary links, so `just ci-matrix`
and `just test-all` die at the compile step before running a test. The crate's library builds fine; only its
harness fails. Verified NOT fallout from phase-361 — reverting the `nros-rmw/alloc` pin reproduces it
identically. Same family as #0618 in a different register: a library assumes the FINAL ARTIFACT provides
something and nothing checks it — lang items there, an `extern "C"` platform symbol here. Note #0420 when
weighing a no-op fallback: a silently no-op log facade was its own bug. See `0619-*`. (2026-08-16)

**#618** (build/api, open 2026-08-16) — `#[panic_handler]` and `#[global_allocator]` are link-time singletons
of the FINAL ARTIFACT, but nano-ros picks them in LIBRARY crates keyed on the PLATFORM — so "exactly one per
image" is not guaranteed by the build, it is maintained by hand at every dep-site. Both halves of #0617 are
the two failure modes this permits: two providers (`#[global_allocator] in nros_platform conflicts with`) and
none (`#[panic_handler] function required`). SIX providers exist under five different gating idioms
(`nros-c`'s spin loop, `panic-halt`, three board crates — one of them UNGATED — and libstd), and the
composition rule lives in prose: `nros-board-nuttx` documents that C/C++ images must take it with
`default-features = false` so `nros-c` can own the runtime. Keying on the platform cannot work — two images
for one platform legitimately want different handlers (print-and-exit for a fixture, log-and-reboot for a
shipped controller), and `nros-c`'s own comment apologises for choosing a policy it cannot know. Direction:
libraries never provide; the IMAGE chooses, in the user's project; the entry layer (`nros::main!` /
`nano_ros_entry()`) supplies a default because it IS part of the final artifact — with one qualification, that
a staticlib IS a final artifact when it is the deliverable, which is what #0615 discovered. Gate worth having:
per image coordinate, count the crates that can emit the lang item under the selected features and require
exactly one. Invisible on host lanes, where `std` supplies both. See `0618-*`. (2026-08-16)

Recently resolved (2026-08-15): **#610** — `just zephyr setup` downloaded `zephyr-sdk-0.16.8_linux-x86_64.tar.xz`
on ANY host. `scripts/zephyr/setup.sh` hardcoded the tarball AND its sha256, so on aarch64 the fetch and the
CHECKSUM both succeeded and the SDK's own installer then died with `Installing host tools ... ERROR: Host
tools installation failed` — naming neither the architecture nor the tarball, 1.3 GiB in. The cross
toolchains are host-agnostic; the HOST TOOLS inside the tarball are native binaries, which is why only the
install fails. Worse than the usual #0582 shape because the passing checksum argues the download was right,
pointing suspicion at the SDK release instead of the request. Fixed: tarball + sha256 selected together from
`uname -m` (upstream ships linux-aarch64; the x86_64 sum is byte-identical to the old hardcode, confirming
the source), unmapped arch is a hard error naming what to add. Reached via tier 2's lane gate failing on
three zephyr compile-check fixtures. NOTE an earlier tier-2 run reported `== zephyr == OK` on this host with
no west installed — a family reporting OK is not proof its toolchain exists. See `archived/0610-*`.
(2026-08-15)

RESOLVED 2026-08-16 — **#0616** the mixed zephyr entry died on `the #[global_allocator] in nros_platform
conflicts with global allocator in: nros_platform` — one crate named on both sides, i.e. TWO cargo workspace
ROOTS sharing one artifact directory (issue 0493's class, in the Zephyr lane rather than the Corrosion one).
phase-361 W8 made `nros-platform` the sole owner of the lang item, which turned a latent duplication into a
compile-time error instead of a link-time duplicate symbol — louder, not newer. Fixed by deriving
`CARGO_TARGET_DIR` from the cargo workspace ROOT (`cargo locate-project --workspace`, not a path compare —
`packages/cli` is a separate workspace inside the repo), plus a configure-time FATAL_ERROR when two roots
claim one directory. See `archived/0616-*`.

RESOLVED 2026-08-16 — **#0621** a VENDORED nano-ros splices its 272 example packages into the CONSUMER's
package index. `build_pkg_index` walks the consumer's root, descends into the nano-ros subdirectory and dies
on the first duplicate — `demo_bringup`, naming two directories inside the dependency for a package the
consumer never asked to build. The 28 duplicated names are CORRECT: every copy-out workspace has its own
`demo_bringup` (x18) / `native_entry` (x12) / `talker_pkg` (x8), because RFC-0026 + RFC-0066 give the same
role the same name deliberately, and they are unique per workspace — the only scope where uniqueness means
anything. Fixed with `.nros-ignore` at the repo root, whose semantics fall out of the walker itself: the
filter returns true for `depth() == 0` before reading any marker, so the file is never consulted when
nano-ros IS the workspace root and prunes at depth 1 when nested. Also renamed three packages that really
did collide — `native_talker` was declared by `talker` AND both custom-transport examples, one of them a
listener — to `native_custom_transport_{talker,listener}`; fixing those alone only moved the failure to the
next duplicate, which is what showed the duplicates were not the disease. Found bumping
`nano-ros-rt-eval`'s pin for phase-358 W3's cells. See `archived/0621-*`. (2026-08-16)

RESOLVED 2026-08-16 — **#617** embedded link failures from phase-361's opt-in-features direction: a `no_std`
FINAL artifact needs one `#[global_allocator]` and one `#[panic_handler]`, and a HOST build detects neither
missing (`std` supplies both). Fixed: the C++ mapper arm gated on `nros-cpp/alloc` while its variant is
gated on `nros-rmw/alloc` (`e5bc6363e`), and `platform-nuttx` selecting NO malloc and NO panic provider
where zephyr/freertos/threadx select both — invisible while NuttX linked `std` (`717030676`, gated by
`check-platform-provider-features`). The third item did NOT belong: the duplicate `nros_platform` allocator
is a DUPLICATED crate, not an under-provided one — issue 0493's class, fixed as #0616; phase-361 only made
it louder by giving `nros-platform` a lang item. See `archived/0617-*`.

Recently resolved (2026-08-16, `7f4da362c`): **#0614** — `cargo check -p nros-c` (and `-p nros-cpp`) failed
with two errors naming neither the crate's feature contract nor the fix, after phase-361 W3 correctly stopped
these crates defaulting to `std`. Both build `staticlib`/`cdylib` — FINAL artifacts — so a bare check failed on
a dependency rather than on anything the caller wrote. Fixed three ways at three levels: `default =
["panic-spin"]` (a complete configuration without a `std` default — consumers take these `default-features =
false` anyway); `[profile.dev] panic = "abort"` for the STRATEGY error, which cargo accepts only per profile so
no feature could have fixed it, and which matches what every shipped image already does; and
`nros_cpp_time_ns`'s no_std arm reaching `nros_platform_clock_ns` instead of `ConcretePlatform` — a
compile-time requirement for a LINK-time fact. All four invocations in the issue's table re-verified green.
Worth noting the issue filed itself as discoverability with three doc/`compile_error!` options, and cause (2)
was reachable by none of them. See `archived/0614-*`. (2026-08-16)

Recently resolved (2026-08-16): **#615** clause (d) reasoned only about DEP-SITES, so it called
`nros-cpp`'s `default = ['panic-spin']` unreachable and asked for it to be emptied — which BREAKS the
build, since `nros-cpp` is a staticlib and a `no_std` final artifact needs a panic provider (verified:
`error: #[panic_handler] function required, but not found`). A crate whose own build is final is a
consumer of its own default. Fixed by exempting those, REPORTING the exemption rather than skipping
silently, and self-testing both directions — a staticlib default passes, an rlib-only one still fails.
See `archived/0615-*`.

**#602** — `[source.threadx]` declares `eclipse-threadx/threadx` at `4b6e8100` while `.gitmodules` declares the
`NEWSLabNTU` fork and the gitlink records `13d061a7` (whose parent IS `4b6e8100`; the commit between them is our
LP64 fix). Filed first as "provisioning reverts the fix" — REFUTED: `provision()` returns `Submodule` whenever
`submodule` is set, that arm runs `git submodule update --init` against the GITLINK, and `git`/`ref` are read only
in the Clone arm. The fields are inert here. What survives is a data file naming a push target we do not use — the
clone's `origin` really was upstream, which is where the vendored-fork workflow would have pushed. Index fixed;
a gate for it was written and deliberately DROPPED (it would police fields nothing reads). Five siblings drift the
same way. Open question: what actually moved the checkout, and whether these fields belong at all.
See `0602-*`. (2026-08-15)

Recently resolved (2026-08-15): **#603** — `nros setup --system --check` reported `libmbedtls` PRESENT on a
host carrying only the RUNTIME package, so `just build-test-fixtures lane=tier2` built all six embedded
families (~20 min, every one OK) and then died in the native lane on a missing `mbedtls/entropy.h`. The
entry mapped `apt = ["libmbedtls-dev"]` but probed `sharedlib = "libmbedtls.so"`, which PREFIX-matches the
`libmbedtls.so.14` that `libmbedtls14` ships — headers and the unversioned symlink are what `-dev` adds. The
gate asked "is the runtime loadable", the build asked "can I compile against it" (issue-0196's shape), and
the one command a user runs FIRST to avoid a 20-minute dead end told them they were fine. No probe kind
could express it: `pkg_config` is the documented dev probe but Ubuntu's `libmbedtls-dev` ships no `.pc` —
which is exactly why `generate_mbedtls_pc_files` fabricates one (#0399). Added a `header` probe kind
(include spelling, `/usr/include` + `/usr/local/include` + multiarch, no compiler invoked) and moved
`libmbedtls` + `libz3` onto it. **The sweep's real finding: the rule is what the CONSUMER needs, not what
the package is named.** `libclang-dev` looks identical and was deliberately LEFT on `sharedlib` — bindgen
`dlopen`s libclang, and this host has `libclang-{12,14}-dev` working with NO `/usr/include/clang-c` (versioned
dev packages install under `/usr/lib/llvm-14/include`), so a header probe there would be a false negative
that hard-blocks setup. `libslirp` is the other control (runtime package, versioned SONAME, correct). See
`archived/0603-*`. (2026-08-15)

Recently resolved (2026-08-15): **#0609** — fifteen zenoh interop / workspace-feature / multi-node tests failed
as `rmw_zenoh_cpp: Unable to make PublisherData … the 'timestamping' setting must be enabled`, before their own
assertions ran. TWO causes, fixed by different hands. (1) `ZENOH_SESSION_CONFIG_URI` REPLACES the shipped
`DEFAULT_RMW_ZENOH_SESSION_CONFIG.json5`; `write_zenoh_session_config` restated only `mode`/`connect`/`scouting`,
so `timestamping` was dropped and — `/rosout` being transient-local — NO ROS 2 node could start. Now parses the
shipped JSON5 and OVERLAYS our three settings onto it; restating one key would have left the mechanism.
(2) The rest was the version skew: `ros-humble-rmw-zenoh-cpp` 0.1.1 → 0.1.9 moved the vendored zenoh
**1.2.0 → 1.8.0** against our `zenohd` 1.7.2, and delivery works — 1.2.0 ↔ 1.7.2 established a session and
delivered nothing. The pinned overlay was NOT needed. Verified `interop_e2e` 10/10. Still open elsewhere:
nothing reports which zenoh a run uses, and the number lives in `zenoh_configure.h`, not any package version.
See `archived/0609-*`. (2026-08-15)

RESOLVED 2026-08-16 — **#606** `[deploy.*].board` carries the DOWNSTREAM ecosystem's board id, not a
nano-ros descriptor name: of 19 distinct values, the 5 nothing claimed were Zephyr's `native_sim/native/64`,
PlatformIO's `esp32dev` and NuttX's `qemu-armv7a-nsh`/`nuttx-qemu-{arm,riscv}`. The other 14 only looked
fine because they happen to be spellings a descriptor also uses. Fixed by having each descriptor CLAIM the
downstream spellings it covers, plus ONE rule in `resolve_deploy` (names, then the directory alias, then
platform) — which let the three ad-hoc fallbacks be deleted. Gate: `check-deploy-board-resolves`
(mutation-verified) fails a value that resolves to zero or several descriptors. See `archived/0606-*`.
(2026-08-16)

RESOLVED 2026-08-16 — **#605** phase-351 W5's Zephyr arm was INERT for the Rust entry lane: that cargo is
spawned by zephyr-lang-rust's `rust_cargo_application()`, which the hook in `nros_cargo_build()` never
reached. Fixed in three parts — the module resolves the facts once at module scope (from
`APPLICATION_SOURCE_DIR`, an entry leaf); `cargo-features-patch.sh` gains a hunk injecting
`${NROS_BOARD_FACTS_ENV}` BEFORE `cargo` (`cmake -E env` ends the env at the first non-KEY=VALUE arg) that
fails loudly if upstream moves that line; and `board-facts` accepts the leaf shape those examples use
(`[package.metadata.nros.deploy.<key>]` with no `entry` stanza). Verified in the GENERATED ninja, not just
the configure message. Still unmeasured: the C/C++ Zephyr cells, which #0590 stops before they configure.
See `archived/0605-*`. (2026-08-16)

**#612** (testing, open 2026-08-16) — `tests/signal_fd_wake.rs` cannot LINK in any configuration and no
lane enables the feature, so `signal-fd-wake` has never been runtime-tested. `nros-node` carries no platform
provider in its dev-dependencies, so the moment the test's feature set pulls `NodeWake` in, every
`nros_platform_wake_*` is undefined; sibling test binaries link only because nothing references them.
Independently, `grep -rn signal-fd-wake just/ .github/ scripts/` finds one comment and no runner. Issue
0577's class, and the instance phase-359's doc says to budget for — it surfaced when W10 ported the
forwarder off `std::thread`, which is therefore compile-verified only. Fix needs BOTH a platform in
dev-deps and a lane; loosening the test's `#![cfg]` until it compiles would restore the appearance of
coverage without the substance. See `0612-*`. (2026-08-16)

**#608** (testing, open 2026-08-15) — a fixture row built into a phase-340 shared cargo GROUP is
resolved at the AMBIENT cargo profile, discarding the platform carve-out the caller just applied. NuttX
Rust pins `nros-minsizerel` (the `lto=off` cross-CGU bug), and `binaries/nuttx.rs` honours that on the
LEAF path — but the group branch in `binaries/mod.rs` formats its path with a bare
`cargo_target_profile_dir()`, so every group-built NuttX Rust row is looked up under
`nros-relwithdebinfo`, which the builder never writes. Presents as the 0584 "broken promise" panic
naming a binary that exists one directory over. issue-0196's shape once more: a rule threaded through
the leaf resolver and the staleness probe but not through the third consumer phase-340 added beside
them. Check `freertos-qemu`, which has the identical carve-out. NOT caused by phase-359 W7 (every file
involved is byte-identical to HEAD; W7 only changed the feature signature, so the group dir was newly
named and the mismatch became visible). See `0608-*`. (2026-08-15)

Recently resolved (2026-08-16): **#590** — every cyclonedds cell failed to compile: `ddsrt`'s POSIX environ TU
calls `setenv`/`unsetenv`, which Zephyr's libc declares only behind CONFIG_POSIX_API, and gcc 14 makes an
implicit declaration fatal. Fixed with a Zephyr `environ` backend + TU swap, NOT by enabling the POSIX surface
— that surface is pooled (#0371/#0496) and is what the native sync backend exists to escape. The lane's real
cost was that FOUR more walls stood behind this one, each invisible until the previous cleared (task probes
naming `pthread_t`, the rosidl dead path, a match arm gated on the consumer's `alloc`, and #0616's shared
corrosion target dir) — three of them collisions between commits that were fine apart. `just zephyr
build-fixtures` now completes: 0 failures across all 70 leaves. Unblocks phase-353 W2, greens phase-363 W5.
(The old row pointed at `0596-*`, a different issue's file.) See `archived/0590-*`.

Recently resolved (2026-08-15): **#588** — WITHDRAWN, premise wrong in three ways, recorded because the
misreading is the useful part. I claimed `build-test-fixtures lane=native` "builds nothing while stamping
the lane": the `build=0` line belongs to a DIFFERENT builder, and the native stage's output is redirected to
`tmp/build-test-fixtures-*/native.log`, which shows the row building normally with all three RMW variants
and zero errors. The MISSING binary was a test naming a ROS NODE (`add_two_ints_server`) where the crate's
only `[[bin]]` is `service-server` — fixed upstream the same day by `c385a914a`, independently. The fifth
failure was a capability skip (`[SKIPPED] qemu-baremetal-main-e2e fixture not prebuilt`) that only reads as
FAIL under bare `cargo nextest`. Lesson kept: `find` failing to locate the artifact was evidence the NAME
was wrong, not the builder — check the per-stage log before concluding anything about a build.
See `archived/0588-*`. (2026-08-15)

Recently resolved (2026-08-15): **#587** — `check-cargo-config-tracked` called six threadx-linux configs
"pure sync output" and told you to `git rm --cached` them; the comment IS the file (issue 0582's finding on
why there is deliberately no `[build] target`). Its predicate excluded ALL comment lines. Fixed by excluding
only the decor sync itself emits (`# === BEGIN/END nros-managed …` and the trailing `# nros-managed`), so
any other comment counts as authored — no heuristic needed, because sync's output is fixed and knowable.
The open question is answered and reframes it: ALL 74 tracked leaf configs score "pure" under the old
predicate, not six; the other 68 were masked by a second condition (they include a committed board
projection) which the threadx six lost in #582's edit. So an allowlist would have been wrong — 68 files were
one upstream edit from the same false positive. Verified both ways: a synthesised pure config is still
caught. See `archived/0587-*`. (2026-08-15)

**#601** (build, open 2026-08-15) — a COLD cyclonedds fixture build dies `code=127` on
`/opt/ros/humble/bin/idlc: error while loading shared libraries: libiceoryx_binding_c.so`. The tool is
present; it cannot LOAD, because ROS's library path is not in the BUILD's environment. `find_package`
takes the first prefix that resolves, so ROS's copy beats the `build/cyclonedds` that
`just cyclonedds setup` provisions — selection by EXISTENCE where the property is RUNNABILITY. Invisible
until something makes the leaves cold (`just setup-cli` does, by design), which is why the lane looked
healthy all day and then could not rebuild. Third instance of this shape in one session (see #0500, and
the rosidl-adapter ladder fixed alongside). See `0601-*`.

Recently resolved (2026-08-15): **#562** `nros sync` rewrote byte-identical files, restamping mtimes and
buying a cmake reconfigure for no change. The class fix (ONE write-if-changed helper in
`cargo_nano_ros::atomic_file`, re-exported by `nros-cli-core`, replacing four private spellings and
reaching the five writers that had none) had landed without the status being flipped. Both headline
measurements re-verified on no-op syncs: `examples/native/rust/talker` 2 -> **0** restamped,
`examples/workspaces/features` 27 -> **6**, and all six are cmake's own outputs under the probe's
`build/` — sync-owned restamps are **0**. The first measurement attempt reported 31890 files because it
ran `comm` on unsorted input, which warns and continues; the numbers above come from a stat-map compare.
NOT closed by this: the threadx leaf's `.cargo/config.toml` whitespace churn is a CONTENT difference
write-if-changed cannot suppress, and is recorded in phase-353 W1. See `archived/0562-*`.

Recently resolved (2026-08-15): **#585** — the ThreadX-Linux logging smoke fixture booted, entered the
Rust entry, exited 0 and emitted NONE of its six log lines. Root cause: the board's log writer hardcoded
`const SYS_WRITE: isize = 1`, and the Linux syscall number is per-ARCHITECTURE — `write` is 1 on x86_64
and **64** on every asm-generic port (aarch64/riscv64/loongarch64), where 1 is `io_destroy`. Off x86 it
issued an unrelated syscall, which failed, and the return was discarded: no error, no partial output, a
silently mute image. A seventh instance of #0582's class. The raw `syscall` had to stay (the ThreadX Linux
port defines a WEAK `write` that never reaches host fds); only the number was wrong, and it is now
cfg-selected with `compile_error!` for an unmapped arch — guessing yields silence, not a failure. TWO
false trails recorded in the issue: it is not bisectable against #0582 (before that fix the tree does not
link on aarch64 at all), and `+whole-archive` on `libglue.a` was the leading hypothesis and was FALSIFIED.
`nm` on the linked image — one command — showed every symbol present and killed the link-class theory that
had cost a whole detour. Lesson: when a program runs to completion and prints nothing, establish that the
code is PRESENT before theorising about why it is not reached. See `archived/0585-*`. (2026-08-15)

**#582** (build, open 2026-08-15) — the host is assumed to be `x86_64` in six places, and five of the six
fail SILENTLY. Three spellings of one mistake: (1) `c_char` is `u8` on ARM and `i8` on x86, so
`ptr as *const u8` is correct on x86 and a `-D warnings` clippy failure on ARM — `.cast::<u8>()` is the
idiom that compiles identically on both, and the pre-existing `#[allow(unnecessary_cast)]` in the zenoh
service shim is the #326 pattern (a second idiom where a shared one belonged); (2) `rust-lld`/`llvm-ar`
live under the HOST triple's rustlib dir, and two `find_program` lookups hardcoded x86_64 **with
`NO_DEFAULT_PATH`**, so off-x86 the result is an empty variable rather than an error; (3) six threadx-linux
leaves + two `fixtures.toml` rows spelled "host build" as a literal triple, which means "cross compile"
on every other machine. Plus vendored ThreadX keying LONG/ULONG and `ALIGN_TYPE` on `__x86_64__`, where
the `ALIGN_TYPE` arm truncated every pointer a byte pool stores — heap corruption with no diagnostic.
Fixed via `NEWSLabNTU/threadx` `nros-lp64-ulong` + `b52acd8cf`. All three sites of (2) now share ONE
helper, `nros_host_rustlib_bin()`, placed in the cross-RTOS layer because the third caller is
`cmake/toolchain/riscv64-threadx.cmake` and a toolchain file cannot reach an RTOS-specific module — the
first pass put it in `nros-threadx.cmake` and left the toolchain file, which is the #326 shape exactly.
That toolchain also no longer SKIPS its lld setup when the lookup comes back empty (a `FATAL_ERROR` now):
the silent skip is precisely why the hardcoded triple survived — it degraded to GNU ld and failed later
with a message naming neither. **Noted, not fixed:** the 2026-07-28 audit already recorded this defect
(A1/A4, both sites, the exact consequence) and nothing acted on it for a year, because on an x86 host
every symptom here is invisible. Also surfaced an unrelated link defect, fixed here: the ThreadX platform/kernel archives
needed `+whole-archive` because their consumers arrive bundled inside the zpico-sys rlib and land after
them on the link line. See `0582-*`. (2026-08-15)

Recently resolved (2026-08-15): **#472** thirteen of fifteen opaque-storage macros had no compile-time
size check, so a wrong probe was a short buffer rather than a build error. Fixed by `76a787b46`; the row
below was left in the open spelling when the file was archived, which had `check-issue-index` red on main.
See `archived/0472-*`.

Recently resolved (2026-08-15): **#464** the size probe's two silent fallbacks (a polling race, and
committed `NUTTX_FALLBACK_SIZES` constants that had rotted ~11 % BELOW the real size) were removed and
verified incl. NuttX on 2026-08-07, leaving only the half this issue's status line tracked: the macros
that had no compile-time check to catch a wrong size. #472 closed that — all fifteen `*_OPAQUE_U64S` now
assert against `size_of` of the type they store, and `check-opaque-storage-guards` reports
`15 macro(s) emitted, all guarded`. A silently substituted size is now a build error naming the macro.
See `archived/0464-*`.

Recently resolved (2026-08-15, before it was filed): **#0597** — phase-359's `std` census counted only
`cfg` sites where `feature = "std"` sat IMMEDIATELY after `cfg(` / `cfg(not(`, so every `all(...)` /
`any(...)` spelling was invisible: 69 of 252 sites (27 %) in its own scope, 55 in `nros-node`, the
campaign's largest work item. Fixed UPSTREAM by `a9d54004e` (phase-359 W2) hours before this file
existed, and found from the other end — W2 deleted four cfg lines and the gate did not move, "the one
thing a ratchet must never do". Matcher now takes any nesting; baseline re-measured 181 → 242 cfg / 425
→ 421 path in the same commit. Archived for the independent derivation, which carries a mutation test
upstream's does not: two planted `std`-conditional items with no `std::` path leave the gate green,
while a third naming `std::string::String` is caught by the PATH metric — the two metrics are not
redundant, which is why the `cfg` half had to be fixed and not leaned on. Still open for phase-359:
`not(std)` and positive `std` sites are summed, so a conversion between them reads as no progress.
See `archived/0597-*`.

**#589** (zephyr, api-cpp, open 2026-08-15) — on `native_sim` ANY Rust `println!`/`eprintln!` recurses
forever and SIGSEGVs the image: Zephyr's `stdinout_write_vmeth` is `return zvfs_write(1, buffer, count)` under
`CONFIG_BOARD_NATIVE_POSIX`, called FROM `zvfs_write(1, …)` — no termination, `k_mutex` is recursive so it
never deadlocks, just exhausts the stack (`lock_count = 104756`). C/C++ `printf` uses picolibc's console hook
and is unaffected, which is why it stayed latent; the config is IDENTICAL in cells that pass, so it is armed in
every native_sim image and fires only when a Rust std stdio call is reached. Found when #0557's fix routed an
error through #0436's `eprintln!("nros: NodeError::…")` — `x/s buf` in the backtrace is that literal. Worked
around by gating that one site on `not(feature = "platform-zephyr")`; 5 more `std::eprintln!` sites in
`nros-cpp` are the same landmine. See `0589-*`. (2026-08-15)

Recently resolved (2026-08-15): **#586** — the C++ FFI discarded the backend error at 15 sites
(`Err(_) => NROS_CPP_RET_TRANSPORT_ERROR`), so a caller saw "transport error" for a too-long name, a
too-small buffer, an unsupported op or an incompatible QoS — and on a guest the return code is often all that
reaches the console (#0589 makes printing fatal on Zephyr native_sim). Types came from the COMPILER, not from
reading call chains: `Err(e) => { let _: () = e; … }` makes rustc name all 15 at once, including 4 behind
`lending`/`safety-e2e` that a default build never reaches — 5 `NodeError`, 10 `TransportError`. New
`transport_error_to_cpp_ret` sibling; both mappers now EXHAUSTIVE (no `_`, rustc enforces), so a new variant
fails to compile until someone maps it. Gate `check-cpp-ffi-error-mapping` matches only the discard into the
catch-all — its first version flagged 43 sites, most correct, which would have taught people to ignore it.
See `archived/0586-*`. (2026-08-15)

Recently resolved (2026-08-15): **#583** — the nuttx-arm Rust realtime boot tier "stopped scheduling" after
spawning. Not scheduling and not nano-ros code: the image linked a `std` built 2026-08-10 against crates.io
`libc`'s 20-byte `pthread_attr_t` while NuttX's is 56, so `pthread_attr_init`/`destroy` wrote 36 bytes past
the attr on `Thread::new`'s own frame and the caller returned to ~0 (PC walking 0x48, 0x4c, … with `lr == sp`).
That is #0570's defect, whose fork fix (5 -> 14) never reached these artifacts: the workspace signature hashed
sources/tool/resolver but NOT the vendored libc, and the rows set `skip_probe = true`. Fixed both halves — the
signature hashes the pin, and `nuttx-libc-pin-guard.sh` DROPS the build-std artifacts when it moves (an honest
stamp only re-runs cargo, which reuses the `std` it has). Also installed #0572's stdout panic hook on
`run_tiers`, the sibling-spawning path that never had it. See `archived/0583-*`. (2026-08-15)

Recently resolved (2026-08-15): **#579** — `apply_tier_priority` was called only from the SPAWNED tier path,
so the NuttX boot tier kept the init task's priority and its declared `[tiers.*.nuttx] priority` was parsed,
baked, carried to the board and dropped — silently inverting an ORDERING on the one tier that also drives the
shared session flush. Fixed by adopting it on the boot path too (the answer ThreadX already takes; Zephyr
avoids the problem by sorting, #0251). The e2e cell that should have caught it asked only whether the accept
marker appeared ANYWHERE, which one spawned tier satisfied for the whole image — now `EachTierOrFailNote`,
per declaring tier and per declared value. Runtime proof was blocked behind #0583 and landed with it: the guest
prints `tier priority set tier=\`high\` prio=110` and a ~10:1 spin ratio. NOT #0572's cause — that was #0570's
mirror overflow. See `archived/0579-*`. (2026-08-15)

Recently resolved (2026-08-15): **#577** — `cargo test -p nros-node --lib` failed
(`violations_beyond_the_ring_are_counted`, `ExecutorFull`) while `just ci` was green. TWO defects. The test
registered `MAX_VIOLATIONS + 4` = 12 timers against a `MAX_CBS` whose default has been 4 since 2026-03, so
it panicked on the fifth — it landed 2026-08-11 and had NEVER passed. It survived because no lane ran it:
`test-all` is `nextest --workspace`, which resolves `nros-node` WITHOUT `std` (a dependent takes it
`default-features = false`, the 0270 carve-out), compiling out all seven `#[cfg(feature = "std")]` tests in
`executor/tests.rs`. Proved with the same filter against both builds — the workspace listing is empty, the
`-p nros-node` listing has them — and corroborated against a tier-1 sweep where 152 `nros-node` cases ran
and none were these. Fixed by making the test `MAX_CBS`-independent (one timer, repeated stalls; measured
`dropped=4`) and adding `check-node-std-tests` to `check-build` beside `check-cli-tests`, which it mirrors.
`packages/api/nros` checked and is NOT affected (it GAINS tests under `--workspace`). NOT done: a general
gate that every per-package test also exists in the workspace build. See `archived/0577-*`. (2026-08-15)

Recently resolved (2026-08-14): **#573** eleven `zenohd` orphans were alive on the dev host with no test runner
running, the oldest 3.8 days. Two private `RouterHandle` copies (`nros-rmw-zenoh`'s `cffi_smoke.rs` and
`status_events_matrix.rs`) spawned zenohd with a bare `Command::spawn()` instead of the shared
`ZenohRouter`, so they armed no `PR_SET_PDEATHSIG` — and they held it in a `static OnceLock`, whose
`Drop` Rust never runs, so each leaked a router on every CLEAN run too. Their `impl Drop` was dead code
that read like cleanup. Both also re-introduced the bind-port-0-then-close race #470 had removed. The
fixture had been hardened twice (#470, #388) and both hardenings reached only the call site everyone knew
about. Fixed by deleting both copies for `ZenohRouter::start_unique()`; gate `check-zenohd-spawn-sites`
(verified to fail on the pre-fix tree) keeps it the only spawner. Sweep:
`git grep -ln 'zenohd_binary_path' -- '*.rs'`. See `archived/0573-*`.

Recently resolved (2026-08-14): **#0572** — `nuttx-arm/rust` delivered ZERO samples on its 10 ms `/ctrl` tier while
the 100 ms `/telem` tier worked. Third symptom of #0570's one bad write: `pthread_attr_init`/`destroy` put NuttX's
56-byte `pthread_attr_t` into the fork's 20-byte mirror, and on arm the 36-byte overflow lands on
`Thread::new`'s pushed `{r4, r5, r6, r7, r9, lr}`. `high` is the BOOT tier, i.e. the CALLER of `Thread::new` — so
the corrupted tier is the one doing the spawning, and `low` on the new thread is untouched. That is why it was
zero and not slow. Fixed with `__PTHREAD_ATTR_SIZE__` 5 -> 14; `realtime_tiers_e2e` reports 16 rows ran, 0 skipped,
0 out of lane, all pass. #0569/#0570/#0572 were three issues written from three symptoms of one overflow.
See `archived/0572-*`.

RESOLVED 2026-08-14 — **#571** `realtime_tiers` reported a 12-second tier-1 PASS having run 1 of its 16
rows: `lane-filter.sh native` narrows by NAME, and phase-329 left FOUR consumers as one generically-named
test each over every platform's cells, which no name filter can reach into (issue 0357's finding, one
level deeper). They now narrow their own cell list via `nros_tests::lane_scope::admits` — the run-scope
twin of tier 2's `require_coord_in_lane` — and PRINT what did not run; a run where no row ran is a skip,
not a pass. The timeout half was fixed in parallel by #564. Third defect found on the way: five
`*-realtime-*-port` test-groups had NO live members (every filter named a case consolidation deleted), so
the slirp-port serialization they existed for had been silently off — retired into one
`matrix-consumers-serial` group. Gate: `check-lane-scope-consumers`. See `archived/0571-*`. (2026-08-14)

RESOLVED 2026-08-14 — **#568** every GREEN `just ci` ended in two `error: recipe … failed` lines,
because tier 1's success banner spelt `` `just ci-matrix` `` inside DOUBLE quotes: a recipe line goes to
`sh`, so that is command substitution, and the last act of a passing tier-1 run was to execute tier 2's
lane gate and splice its output into the sentence. The gate fails fast on tier-1 fixtures, which is why
this cost seconds rather than hours — and why nobody looked. Nine sibling echoes across `just/*.just`
already escape their backticks; this was the tenth. Sweep: `grep -n 'echo "[^"]*`' justfile just/*.just`.
See `archived/0568-*`. (2026-08-14)

Recently resolved (2026-08-14): **#567** (rmw) — `_zp_unicast_read` reset its receive buffer on EVERY call,
so it could never return with bytes unread and any budget on its drain loop discarded frames instead of
deferring them. Fixed in the zenoh-pico fork (43ddb0ec) by resetting only when the buffer is empty and
compacting otherwise — smaller than expected, because `_z_unicast_client_read` already coped with
pre-existing buffered bytes. Verified: a 16-frame cap now holds inbound delivery at 274 msg/s where the
same cap previously collapsed it to 10; no regression at idle or 1 kHz. Does NOT establish that a budget
buys cadence — on the resumable path a 16-frame cap left stalls unchanged while a 4-frame cap improved
them only by throttling delivery via TCP backpressure, and second reps of both landed in a degraded regime
that also appeared in unrelated cells, so the 2 kHz harness is bimodal and those cells are not evidence.
#506's device half is implementable; whether it is worth implementing is open. See `archived/0567-*`.
(2026-08-14)

Recently resolved (2026-08-14): **#566** (platform-zephyr) — without `CONFIG_POSIX_API` the Zephyr port
stubbed its ENTIRE threading half (~20 `task_*`/`mutex_*`/`mutex_rec_*`/`condvar_*` functions, each
returning -1 at RUNTIME) on a kernel that has `k_mutex`, `k_condvar` and `k_thread_create` natively. Found
because #531's verification needed a board that had never run the smoke suite. Now implemented against
those primitives, with the caller's opaque storage holding a POINTER to a heap-allocated kernel object —
it has to, since the smallest consumer sizes these from `pthread_mutex_t` (a `uint32_t`) which cannot hold
a `struct k_mutex`; same shape as the FreeRTOS port's `SemaphoreHandle_t`. `k_mutex` is recursive for the
owning thread, which is what `mutex_rec_*` requires. `task_*` still needs `CONFIG_DYNAMIC_THREAD` (a
dynamic thread needs a dynamic stack) but that case is narrow, documented, and no longer drags mutexes
down with it. Verified both arms: `qemu_cortex_m3` smoke PASS (was FAIL at mutex_init), `native_sim`
unchanged. See `archived/0566-*`. (2026-08-14)

Recently resolved (2026-08-15): **#563** — the executor's 88192-byte inline storage is STATIC (`.bss`), but
CONSTRUCTING it cost ~23 KB of stack, because `Executor` itself was 11632 bytes and is returned by value
before being written into that storage. `remap_table` alone was 6664 of those bytes (57%): the SEVENTH
sized table, left inline when phase-271 moved the other six into caller-owned carved storage. Now carved
too — `size_of::<Executor>()` 11632 -> 4992, with NO `ExecutorSizing` change (fixed `MAX_REMAPS` count,
`u64_len()` covers it), so the C/C++ inline storage moves only 88192 -> 88264: the same 6.6 KB relocated
off the constructing thread's stack. Measured on the board that motivated it: at MAIN_STACK 32768, which
previously overflowed for all three, C and C++ now PASS (Rust's path is deeper and still wants more, so the
conf stays at 131072). Guard: `executor_stays_small_enough_to_construct_on_a_stack` (<= 6 KiB, loose on
purpose). Exposed and fixed a second defect: a build script re-runs only when its OWN package changes, so
`EXECUTOR_OPAQUE_U64S` went stale against a dependency's layout change and failed with a const assert
naming neither cause nor remedy — `nros-sizes-build` now watches rustc's own depfile for the probe rlib.
See `archived/0563-*`. (2026-08-15)

RESOLVED 2026-08-13 (SPLIT into #569/#570) — **#565** — NuttX **Rust**, BOTH arches: the 100 ms low tier was reported as never scheduled, so `/telem` never reached 5

Recently resolved (2026-08-14): **#0569** — `nuttx-arm` Rust `realtime_tiers_e2e` aborted with
`Transport(ConnectionFailed)` after the entry banner. NOT a transport bug: same root cause as #0570 — the 56-byte
`pthread_attr_init`/`destroy` write into a 20-byte mirror, which on arm lands on `Thread::new`'s pushed
`{r4, r5, r6, r7, r9, lr}`. Its own note said "do not assume a shared fix" with #0570; the note was right to demand
evidence and wrong about the answer. 16/16 rows pass with no transport change. See `archived/0569-*`.

Recently resolved (2026-08-14): **#0570** — the vendored NuttX `libc` fork mirrored `pthread_attr_t` as 20 bytes
(`__PTHREAD_ATTR_SIZE__ = 5`, the `CONFIG_SCHED_SPORADIC=n` layout) while both boards set `=y`, making it 56. Every
`pthread_attr_init`/`destroy` wrote 36 bytes past the object `Thread::new` had on its frame: riscv returned to
address 0, arm corrupted `{r4..r9, lr}` (#0569, #0572). Fixed 5 -> 14 on the fork's `nuttx-0.2`; gated by
`check-nuttx-libc-struct-sizes`, which measures every mirrored type against the built NuttX headers — #0167 fixed
the same class for `pollfd` and left no rule behind. 16 rows ran, 0 skipped, all pass. See `archived/0570-*`.

Recently resolved (2026-08-14, REFUTED): **#0575** — filed as "the libc pin is on no remote branch, so a clone
cannot resolve it". False. `git branch -r --contains` answers about the remote-tracking refs THIS clone happens to
have, and the submodule is shallow with a `main`-only fetch refspec, so it could see nothing else. `git ls-remote`
shows the pin published on `nuttx-0.2` since 2026-07-11. Use `ls-remote` for publication questions; `--contains`
reads identically whether a commit is unpublished or merely unfetched. See `archived/0575-*`.

RESOLVED 2026-08-13 — **#565** filed as one bug ("the 100 ms low tier is never scheduled, both arches") on the
strength of a verdict that INFERRED its cause from missing telemetry and then killed the guest unread. Both leads
it opened with — the Rust `run_tiers` path, a clock unit/resolution error — are refuted by the console for BOTH
rows. Closed as SPLIT into #569/#570; nothing here was repaired. The lasting change is `d97c9c606`: the failure
now drains the guest before killing it and prints the last 25 lines, or says outright that the guest printed
NOTHING. That is issue #0445's rule one test over, and it separated two unrelated defects on the first run. An
assertion that infers a cause and discards the evidence will merge every failure sharing its symptom.
See `archived/0565-*`. (2026-08-13)

RESOLVED 2026-08-13 (SPLIT into #569/#570, see above) — **#565** — NuttX **Rust**, BOTH arches: the 100 ms low tier is never scheduled, so `/telem` never reaches 5
deliveries. 14 of 16 `realtime_tiers_e2e` rows pass — every zephyr/freertos/threadx/native cell, AND the NuttX
C/C++ cells on the same boards — so the suspicion is the Rust `run_tiers` path (`QemuArmVirt::run_tiers`,
`QemuRvVirt::run_tiers`) rather than board tier plumbing. The 10 ms tier is observed fine, so it is not the tier
mechanism in general; a 100 ms period is exactly where a clock unit/resolution error strands a timer while a
10 ms one keeps firing, and RFC-0073 / phase-352 W6 landed the same week — cheap to rule in or out first (see
#0532). Adjacent and resolved: #0263, same family (spawned NuttX Rust tiers), different failure (wrong priority
vs no scheduling). **Not necessarily new** — it hid behind #0564's truncation and may have failed for as long as
that budget was wrong, so a bisect must not read a killed run as PASS. See `0565-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#564** the one red in an otherwise clean tier 1: `realtime_tiers_e2e` TIMEOUT at 60.003 s,
reproducing solo and printing nothing (the test installs an empty panic hook to classify rows itself, so silence
is by design, not a hang). It is #0413's consolidation cost one binary further out — phase-329 W1 folded 15
per-cell files into ONE test iterating every RealtimeTiers cell in a single process, and the timeout budget did
not follow; it had `[test-groups.*]` for baked-port sharing but no `slow-timeout` at all, so it ran on the default
30 s × 2. The trigger was fixture COVERAGE, not code: these cells `skip!` fast when their fixtures are absent, so
on a partial tree it finished inside 60 s — after a full `build-test-fixtures` every row actually BOOTS a QEMU
image. Worse than slow: rows run IN ORDER and the kill lands mid-sweep, so later cells were never run while the
verdict said TIMEOUT — #0445's absorbing-verdict shape in another mechanism, and it had been hiding two real
failures (#0565). Budgeted `period = 180s, terminate-after = 3` against two measured runs of 127 s and 204 s; the
variance is why the window is generous rather than snug. A consolidated matrix test's wall clock scales with how
many cells have FIXTURES, not how many it declares — so a timeout that always passed can fail the day someone
finally builds the full matrix. See `archived/0564-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#525** the NuttX shared-tree property, closed on its last correctness gap and an explicit
decline of the rest. Directions 1 and 2 landed 2026-08-12 (`nuttx_include_root` + `check-nuttx-shared-tree-headers`,
and `build-nuttx.sh` stating that it guarantees the SNAPSHOT and not the tree). What remained was the SECOND shared
mutable tree this issue recorded as "Not reproduced" — the apps OBJECT tree, where `PREFIX` was empty so objects
landed beside their sources and one arch's could survive into the other's `libapps.a`. Its fix landed as #0488
residue 4 (`PREFIX` from an ARCH-keyed `.nros-build/<CONFIG_ARCH>/`), and it is now MEASURED on the documented user
flow: objects under `.nros-build/arm/`, and zero `.o`/`.built`/`Make.dep`/`.depend` beside the nano-ros sources.
The separation is structural rather than lucky — `$(CONFIG_ARCH)` is a literal path component and the defconfigs
spell it `"arm"` vs `"risc-v"`, while an EMPTY arch is refused outright instead of collapsing to a shared root
(#0551). Only the arm half of the proposed arm→riscv experiment was run: the riscv half reconfigures the shared
kernel tree and leaves it there, and would demonstrate what the path component already guarantees — stated rather
than glossed. Direction 3 (a worktree per arch) is DECLINED, not deferred, on this issue's own reasoning: with 1
and 2 landed the state cannot reach a compile input and no longer surprises the reader, so it buys tidiness for
disk. An issue left open on a direction its own text argues against is tracking a preference, not work.
See `archived/0525-*`. (2026-08-13)

Recently resolved (2026-08-13): **#559** (build) — every `build-test-fixtures lane=native` left a TRACKED
config modified, so `git pull --rebase` refused until you `git checkout` output the next build regenerates.
`nros sync` projects a board's `cargo_config` into `<leaf>/.cargo/nros-board.toml` and adds it to the leaf
config's `include`; the generated file states its own contract — "This file IS committed … a fresh clone
must be able to LINK this leaf before any sync has run". **39 leaves commit both; `threadx-linux/rust/talker`
committed neither**, so sync legitimately rewrote both on every build, and a fresh clone lacked a projection
its board declares (issue 0440's LINK-time gap). Fixed by committing them, matching the 39: sync is now a
no-op, `cargo metadata` exit 0, `check-cargo-config-tracked` OK. **What this does NOT claim:** I first read
it as "three tracked configs include an untracked projection" — issue 0463's hard parse error — which was an
artifact of grepping the string anywhere in the file instead of in the `include` line; checked properly, 39
name it and all 39 have it committed, zero mismatches. talker's five siblings share its deploy and have no
projection at all, not even from an explicit sync, so this is not five more missing leaves; that asymmetry
is a phase-341 question and normalising all six on a guess would be the larger error. See
`archived/0559-*`. (2026-08-13)

Recently resolved (2026-08-14): **#531** (platform-zephyr) — `nros_platform_clock_us()` returned 0 FOREVER on
Zephyr Cortex-M boards at or below 60 MHz: the port read `k_cycle_get_64()`, which returns 0 unless
`CONFIG_TIMER_HAS_64BIT_CYCLE_COUNTER` is set (its `__ASSERT` compiles out in release), and the SysTick
driver only selects that symbol `default y if (SYS_CLOCK_HW_CYCLES_PER_SEC > 60000000)`. The executor
derives its timer delta from this clock, so periodic callbacks never fired while subscriptions kept
working. Fixed by preferring the cycle counter where the board provides one and falling back to
`k_uptime_ticks()` otherwise. **Now verified by running it**: same binary, only the clock function swapped,
on `qemu_cortex_m3` (12 MHz) — pre-fix `clock_ms: 0 -> 0` and FAIL, fixed `0 -> 60` across a 50 ms sleep.
The lane needed two board facts on the way, neither clock-related: no entropy device (new
`cmake/zephyr/qemu-cortex-m3.conf`), and a native_sim-sized heap that overflows 64 KB of RAM. See
`archived/0531-*`. (2026-08-14)

Recently resolved (2026-08-15): **#557** — the Zephyr Cyclone C/C++ action images "failed at boot"
(`rc=-100`). Cause was ONE doubled underscore: `ros_form_to_dds` appended a trailing `_` unconditionally to
`…/Fibonacci_SendGoal_`, which already had one, giving `Fibonacci_SendGoal__`. That mangled tail defeated
#0234's idempotence guard in `action_effective_base` (a `_SendGoal_` suffix test), so the infix was appended
AGAIN — the exact doubled form that guard exists to prevent — and the descriptor lookup missed. Only C/C++ hit
it: `ros_form_to_dds` early-returns for the DDS-form names Rust advertises, which is why C/pubsub, C/service
and Rust/action all passed. Fixed by making the append idempotent. Verified: case_17 C + case_18 C++ + case_16
Rust + case_14 C service all PASS in ~8 s against a 60 s timeout. The six `tid … is in use!` lines were a red
herring (benign, but they leak 32 KB each). Exposed #0586 and #0589. See `archived/0557-*`. (2026-08-15)

RESOLVED 2026-08-13 — **#556** `threadx_riscv64.rs::build_rust_example` hand-spelt
`target-zenoh/<triple>/<profile>/<bin>` onto the example dir. That leaf's row authors no `target_dir`, so
its artifact root is `<dir>/target`: the hand-written path matched NO row, attribution failed, the
shared-group redirect never fired, and the resolver read a **06-13** artifact while the build wrote
`build/cargo-fixtures/threadx-riscv64-<slug>/`. Both `rtos_e2e` ThreadxRiscv64 cases failed as "not
prebuilt" on a lane that reported OK — and were carried as QEMU load-flakes (twice in this session's own
triage) because a stale-path read is indistinguishable from a real failure unless you check where the
binary came from. Fixed by delegating to `build_threadx_rv64_rust_example_rmw`, the sibling already doing
it via `select_row`. Both now PASS (35.3s / 40.3s). Swept: last resolver bypassing the row. #0393 /
#0482's class. See `archived/0556-*`. (2026-08-13)
RESOLVED 2026-08-13 — **#528 / #548 / #555**, closed together on one full-sweep measurement.
`just build-test-fixtures` (lane=all) reports `== zephyr == OK` with 69 fixtures built, zero
`EXECUTOR_OPAQUE_U64S` asserts (was six leaves, whole module down) and zero clock undefined refs —
which is #528's own stated exit condition ("stays OPEN until the zephyr module builds") and both of
#548's criteria, `build-rs-action-client-xrce` being `zephyr-fixture-12` in that run. #548's second
criterion swept by hand too: of 15 first-party C/C++ files naming `nros_platform_clock_{ms,us}`,
twelve include `nros/platform.h` and see the `static inline` wrappers, one IS the defining header, and
two mention the names only in prose. **#555 is the gate #548 asked for** — three arms (use without the
header; a hand-written `extern` declaration, which compiles and fails at LINK; a second TRACKED
definition), comments stripped so the two prose files do not cry wolf. **It does not catch the issue
that asked for it, and that is recorded rather than papered over:** replaying the real pre-fix sources,
#547's `internal.hpp` is caught (3 hits) and #548's `platform_aliases.c` is CLEAN — that file included
the header and declared nothing; what failed was the include RESOLVING to a stale copy on Zephyr's
include path, a property of `-I` order that no source-scanning gate can see. The declaration arm's first
draft read `return nros_platform_clock_ms();` as a declaration; its own self-test caught that.
See `archived/0528-*`, `archived/0548-*`, `archived/0555-*`. (2026-08-13)

Recently resolved (2026-08-13): **#554** (build) — `NROS_FIXTURE_SCOPE=native` demanded four west-built
compile-checks the native lane cannot produce, so `just ci` died at the staleness gate before a single test
ran and `just build-test-fixtures lane=native` could not help. `a12e2c3e4` (#536 / phase-350 W2) added four
west `[[compile_check_fixture]]` rows; `check-fixtures-stale.sh` lists compile-checks with no builder or
lane filter, so every scope demanded every row — while the manifest says outright "Built by the WEST lane
… never by compile-check-fixtures.sh". That is #482's distinction missed for the compile-check inventory:
which fixtures must be FRESH is the lane's cell cover, not every row. Fixed by dropping `west-*` rows when
the scope is `native`; `all` and `coords` still demand them, since silently dropping one there would hide a
real staleness. Verified both directions by counting what each branch passes to the probe — native 0/36,
all and coords 4/40 — and end to end (exit 1 → exit 0). **The predicate is a PREFIX and that was not
obvious:** there are TWO west builders (`west-build` ×1, `west-configure` ×3), so matching the literal
`west-build` would have fixed one of four and left three failing identically; the correction then landed in
the COMMENT only, leaving the awk on the literal, which the gate run caught. Same mistake twice — verifying
the change I meant to make rather than the one on disk. See `archived/0554-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#553** two failures that looked unrelated had one cause, and it is not NuttX-specific.
`_nros_resolve_rust_target` memoized its answer into a permanent `CACHE INTERNAL` entry and short-circuited
on it FIRST, and nothing invalidates it — so whichever scope called it first decided the triple for the whole
build tree, forever, surviving every reconfigure because the memo lives in the cache rather than a target dir
a clean rebuild removes. `examples/workspaces/realtime-cpp/build-workspace-fixtures-nuttx` was configured
host-first and answered `x86_64-unknown-linux-gnu` while its own cache carried
`Rust_CARGO_TARGET:STRING=armv7a-nuttx-eabihf`. One stale string produced BOTH of #551's residuals: the msg
FFI staticlib path embeds the triple, so the glue built under the host triple and ld died with
`libnano_ros_cpp_ffi_std_msgs.a: file format not recognized`; and `nros_nuttx_include_root()` derives the
NuttX arch from that triple, matched neither arm nor riscv, and took its shared-tree fallback — which is why
#551's "fifth site" looked unfixed. There was no fifth site; the include list is generated from the same
`NanoRos` INTERFACE property #551 already fixed, fed a poisoned triple. Fixed by demoting the memo below any
explicit target (and below a `-D` cache read, which crosses `add_subdirectory()` where the normal var does
not), rewriting it on every resolution so it tracks the authoritative answer; corrosion's copies stay BELOW
the memo, since `Rust_CARGO_TARGET_CACHED` was the HOST triple in that very tree and promoting it would be
the same bug facing the other way. Poisoned trees self-heal on the next configure. It survived because
`check-cargo-target-spelling` covered this resolver in seven arms and had NO memo coverage at all — three
added, verified non-vacuous (the first goes red under the old precedence). `just nuttx build-fixtures-arm`
rc=0, 5 artifacts including the leaf that failed the ARM link. See `archived/0553-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#551** — `just build-test-fixtures` took the whole NuttX lane down with `fatal error: nuttx/config.h: No
such file or directory`, and the header was missing because `just nuttx build-integration-app`'s restore
runs `make olddefconfig`, whose `clean_context` DELETES it (`tools/Unix.mk`). `.config` came back
byte-perfect; the tree came back de-contextualized, and `build-nuttx.sh` keys its short-circuit on
HEAD+defconfig+snapshot — all still matching — so it no-ops forever. Restore now runs `make context` too.
But the header should not have mattered: issue 0525 already ruled the shared tree out as a compile input
("this path guarantees the SNAPSHOT, never the tree"), and FIVE build inputs were still reaching it. Its
gate greps for a receiver NAMED nuttx while the rule is about the VALUE, and it scanned neither `.toml`
nor `config/` nor the root `CMakeLists.txt` — where the two sites that actually took the lane down lived
(`"{env:NUTTX_DIR}/include"` in the zenoh-pico manifest, `${NUTTX_DIR}/include` in root cmake, the latter
under a comment that already said "the NuttX EXPORT include tree"). Fixed with a `{nuttx_include}`
manifest token and a cmake `nros_nuttx_include_root()` (arch from the CARGO TRIPLE first — the workspace
lane configures with the HOST compiler while `Rust_CARGO_TARGET` is `armv7a-nuttx-eabihf`), plus a
proximity-scoped taint + `.toml`/`config/`/root-cmake scope in the gate (1365 → 1653 files). Also fixed the
Makefile's parse-time `$(shell mkdir -p …)`, which ran during `apps_preconfig` with `DELIM` and
`CONFIG_ARCH` both empty and created `nuttx-appsexternal.nros-build/`. config.h failures now 0. **Still
open:** a fifth site (`nros-nuttx-ffi`'s cmake-passed include lists) and, newly reachable behind it, a
HOST `libnano_ros_cpp_ffi_std_msgs.a` on realtime-cpp's ARM link line — phase-155's wrong-arch class,
needs its own id. **Both closed by #553 the same day, as one defect** (a stale cargo-target memo poisoning
the arch resolution); there was no fifth site. A gate's SCOPE is part of the rule it enforces.
See `archived/0551-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#550** `just build-test-fixtures` died at leaf 17 on `Cannot find source file:
third-party/dds/cyclonedds/src/ddsrt/src/sync/zephyr/sync.c`, and it was not a code bug: the submodule sat
at `6eb9227` while HEAD records `8601ca6`, 7 commits ahead — including `a09babf`, the commit that ADDS
that directory. The in-tree half of the pair (`nros_rmw_cyclonedds.cmake` swapping the TU under
`DDSRT_WITH_ZEPHYR`) was current, so cmake named a file the stale vendored half does not carry. CLAUDE.md
states the rule twice and neither statement helps at diagnosis time, because the symptom names a FILE, not
the pull that moved the pointer. No gate looked at submodule pointers, and `check-fast` could not: drift is
a WORKING-COPY state, so index and commit always agree in anything you can push. Fixed by
`check-submodule-drift.sh` as the FIRST item of `check-tier-preconditions` — first because its remedy
(`git submodule update`) rewrites source mtimes and re-arms both the CLI stamp and every fixture, so
clearing it later would undo them. Detection is `git submodule status | grep '^+'`; the script adds
DIRECTION, because direction picks the remedy — behind ⇒ fast-forward (and it prints the missing commits,
so `a09babf` names itself), diverged ⇒ REBASE not update (update would strand the local commits detached),
ahead ⇒ OK-with-a-note, since that is the normal middle of the vendored-fork workflow and failing it would
flag every in-flight fork fix. Uninitialized (`-`) is not drift — px4, play_launch layer-3 and nuttx are
deliberately absent. Self-tested both directions. See `archived/0550-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#547** the Cyclone backend hand-declared the platform ABI
(`nros_platform_{clock_ms,sleep_ms,random_u64}`) in three per-platform `extern "C"` blocks, so RFC-0073's
`clock_ms`->`clock_ns` rename (phase-352) compiled fine and failed at LINK: `internal.hpp:63: undefined
reference to 'nros_platform_clock_ms'`. `platform.h` now carries `clock_ms` as a `static inline` shim and
already declared all three, so no hand copy was load-bearing — they only let the file disagree with the
header. Issue 0160's mirrored-FFI class in FUNCTION form: the struct version corrupts a field, this one
fails a link. THIRD breakage from one rename (after #541 and upstream `5dc2fa869`), each visible only once
the previous cleared, because the zephyr lane aborts first-failure. Fixed by including `nros/platform.h`
in the branches that use the ABI — the include stays INSIDE the `#if` guards because the hosted lane
compiles without `nros-platform-api/include` on its path (hoisting it broke `check-rmw-cyclonedds`, and
the comment says so). Verified hosted 17/17 + zephyr leaf clean; swept, one file, now zero hand
declarations. See `archived/0547-*`. (2026-08-13)

Recently resolved (2026-08-13): **#546** (build) — the px4 compile-check codegen'd `generated/px4_msgs` and
then ran `cargo check` WITHOUT `nros sync`, so all three companion leaves resolved
`nros = { version = "*" }` against the public crates.io index and failed. Registry-naming is normal for an
example leaf — dozens do it — because sync writes the `.cargo/config.toml` whose `[patch.crates-io]`
redirects those names at in-repo paths (RFC-0048 W9); the px4 leaves were the ones nobody synced
(`git ls-files examples/px4 | grep -c cargo` → 0, no generated config either), so the CDR bindings the block
exists to type-check never once did. **NOT a host-provisioning gap** — no config exists for these leaves in
the repository, so every checkout hit it; that earlier reading is retracted. Fixed by syncing before
checking, through the script's own existing CLI-resolution idiom rather than a second spelling, and by
making the tally readable: `px4=0` read as "no px4 work to do", `px4=0/3` cannot. Verified end to end
through the real script: **`px4=3/3`**, all three stamped, all three compiling for the first time. Left
alone deliberately: the script still signals failure by withholding a stamp rather than exiting nonzero
(every other lane there does the same, and px4 should not be the one exception), and the `px4_xrce` RUNTIME
tests still need SITL — the #102/#136 debt. See `archived/0546-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#541** `292547dd5 fix(#529, #530)` added a `zephyr` arm to `platform_name`,
reasoning "no behaviour change today: `build_c_shim` is skipped on Zephyr". True, and beside the point:
`platform_name` gates TWO call sites and the other is `build_zenoh_pico_unified`, the whole vendored
zenoh-pico C build. Before #529 Zephyr resolved to `None` and the cargo lane compiled no zenoh-pico C;
after it, cargo compiled the core with the Zephyr platform header and every Rust leaf died on
`zephyr.h:18: fatal error: version.h: No such file or directory` — a GENERATED Zephyr header the cargo
lane has no include path for, because on Zephyr those sources belong to the cmake module
(`zephyr_include_directories`). Same seam as #0460 from the other side. Took the whole zephyr lane, and
so `lane=all`, down. Fixed with `platform_name.filter(|_| !use_zephyr)`: the name stays TOTAL for the knob
ladder, the C build stays off the cargo lane. Reproduced on one leaf and under
`NROS_ZEPHYR_PRISTINE=always`, so not a stale tree. See `archived/0541-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#533** the west fixture lane never resolved its bringups' SystemModels. phase-330
W4.a stopped committing models (correctly — #0380); every consumer had to resolve one instead, and this
lane never learned to. The west build failed at CONFIGURE with "declares system semantics but no
SystemModel was found", but `just/zephyr-ci.just` invokes the script `|| true` (so a missing FVP SDK
cannot fail the lane), which swallowed it: the lane printed "built successfully", wrote no stamp, and
`cli_bringup_zephyr_adapter_shim_boots_native_sim` failed hours later blaming a missing BINARY. Broken
since that commit. `nros sync` at the workspace root cannot help — these fixtures keep packages at the
ROOT, not under `src/`, which sync rejects. Fixed by syncing INSIDE each bringup dir, which is where the
shim's self-pkg resolution looks anyway. Verified from a deleted model + build dir. The `|| true` masking
is NOT fixed and is the second instance after #0510. See `archived/0533-*`. (2026-08-13)

Recently resolved (2026-08-13): **#545** (testing) — TWO core crates could not run `cargo test` at all and a
third test asserted a knob it does not control. `nros-node` failed `-D dead_code` on
`Executor::extra_session_ids`, whose only reader is `rmw-cffi`-gated; `nros-platform-cffi`'s lib test passed
a bare fn where bindgen's `Option`-wrapped function pointer was expected, so the whole crate's tests —
INCLUDING the port conformance suite guarding the platform ABI — had stopped compiling; and
`test_entry_slots_exhausted` hard-coded "MAX_CBS=4" against a BUILD-TIME knob. That last one bites through
directory nesting: cargo reads `.cargo/config.toml` upward, so a workspace vendoring nano-ros and setting
`NROS_EXECUTOR_MAX_CBS` for its own image silently sets it for the submodule too, and the failure reads
like an executor capacity bug. It was misdiagnosed here for a full session as "fails on a clean tree",
because `git stash` does not change the ENCLOSING directory's cargo config. Fixed by scoping the allow to
`not(feature = "rmw-cffi")`, `Some(noop_callback)` with an `unsafe extern "C"` pointee, and deriving the
slot test from `MAX_CBS`. 261 nros-node tests pass with no flags. See `archived/0545-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#549** the Zephyr logging-smoke image had TWO builders and the manifest's one was
unreachable. `zephyr-dev.just` wrote `<workspace>/build-logging-smoke/` (what the test reads) while the
`builder = "west"` row wrote a different dir behind `--include-logging-smoke`, a flag NO build lane passed
— so that leaf was emitted for inventory and never produced. That also explained the `west_bare = true`
anomaly phase-350 W1 declared rather than fixed: no cmake defs, empty staleness signature, because the
leaf was vestigial. Fixed by pointing the row at `build-logging-smoke`, deleting `west_bare` (its only
user), retiring the flag, and deleting both the second recipe and `zephyr-ci.just`'s `want_logging` special
case — whose stated reason ("lives outside the entries loop") stopped being true when W1 gave the leaf a
row. Verified with the build dir WIPED first: the lane produced the image in 119 s and the test passed
against it. See `archived/0549-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#560** a lock pinned by a SUBMODULE's manifest drifts when the pointer moves, and
`nros-launch-resolve` sat unbuildable on main that way (rlm v0.1.6 required, v0.1.4 pinned; `--locked`
refused). Now gated by `check-submodule-pinned-locks.py` in `check-fast`: `cargo metadata --locked
--offline`, **0.25 s, no network** — resolution rather than a build, because resolution is what breaks. The
leaf set is DERIVED (tracked lock + a `path` dep resolving inside a `.gitmodules` path), not listed, so it
follows the next leaf that grows a submodule dep. Verified three ways against the REAL pre-fix lock: green
on the fix, red on the break with the `just lock-update` remedy, SKIP when the submodule is absent. Reason 2 is closed too: `check-launch-resolve-builds` in `check-build` runs the REAL recipe (~14 s warm,
catching link errors a `cargo check` misses), skipping when the submodule is absent so a bare clone can
still `just check`. Split by TIER — lock check sub-second in `check-fast`, compile in the build tier. See `archived/0560-*`. (2026-08-13)

**#604** (build/testing, open 2026-08-15) — cold leaves after every pull, inherited from #509's one
surviving direction: a cold Zephyr leaf costs ~28 s and a pull/rebase/`git stash` was believed to cold every
one. Filed as a MEASUREMENT, not a defect, because the premise may already be half-fixed: content-aware
staleness (a `.srcbaseline` of `(mtime,size,content_hash)` per watched file, shared by both arms since
phase-353 W2) should make an mtime-only change a refresh rather than a stale, and `codegen-fingerprint`
(phase-363) should stop a CLI rebuild invalidating what its codegen did not change. The cost is still being
paid — five separate staleness cascades while chasing one tier-2 verdict on 2026-08-15 — but nobody has
attributed them to genuine input change vs mtime artifact vs tool-fingerprint over-invalidation, and those
want different fixes. Measure and attribute first; do NOT re-run the wall-clock A/B #509 warned about.
See `0604-*`.

RESOLVED 2026-08-15 (phase-353) — **#611** west fixture REUSE never refreshed `.compile-ok`, so after any
CLI rebuild the test-side consumer rejected the fixture PERMANENTLY: `.inputsig` (content + codegen
fingerprint) still matched, so reuse skipped the build, so the stamp kept the old CLI BINARY hash, so
`require_west_fixture` said "built with a different `nros` CLI" — and rebuilding could not clear it because
reuse kept skipping. Three tier-2 tests failed exactly that way on a run whose `lane=all` had just reported
nine families OK. Fixed by re-stamping on the reuse branch, which is the honest claim: reuse is only taken
when the signature matched, and that signature covers what the tool WOULD PRODUCE, a strictly better
question than "same binary". Third instance in two days of the issue-0196 shape (after #574, #576): the
build side writes one stamp, the consumer reads another. See `archived/0611-*`.

RESOLVED 2026-08-15 — **#584** skips were TOLERATED rather than asserted, so `170 skipped` was
indistinguishable from `170 tests silently did not run`. Three parts landed: `skip_class!` gives the marker
a machine-readable class (a 170-skip sweep could previously be classified for 4); an absent in-lane fixture
is no longer a skip at all but a hard failure at the shared resolver; and `check-skip-budget.py` now ASSERTS
the skips at all three `test*` tails — including the success path, since "all failures were skips" is also
what a lane that ran nothing prints. Its two rules are derived, not declared: no `lane` skip for a
coordinate the lane selected, and no skip whose reason is a missing fixture. An expected COUNT per class was
deliberately rejected — counts get edited to match reality on every red. Residue: prefer deselection over
in-test skip. See `archived/0584-*`.

RESOLVED 2026-08-15 — **#527** the doctest phase (and every suite a human re-ran while triaging) overwrote
the rewritten `junit.xml`, so a failed sweep could say HOW MANY real failures it had but not WHICH. Fixed
both cheap ways: `_rewrite-skipped-junit` now snapshots to `junit-real.xml` (a path no nextest run writes),
and `_name-real-failures` prints the ids at all three `test*` recipe tails that previously printed only a
count. Verified on a synthetic junit: count and names agree, and after `junit.xml` is clobbered by a clean
run the snapshot still names the real failure. Filed in the morning and walked into the same afternoon —
a 171-failure sweep read as `tests=1 failures=1` because triage runs had overwritten the evidence. See
`archived/0527-*`.

RESOLVED 2026-08-12 — **#521** `eyre::Context::with_context` on a `Result` is eyre's FEATURE-GATED
anyhow-compat surface (`ContextCompat`); `WrapErr::wrap_err_with` is the native one. `nros-pkg-index` and
`nros-launch-parser` both used the compat spelling while declaring only `eyre = "0.6"`, so they compiled
purely because a `packages/cli` workspace sibling enabled the feature and cargo unified it. Both are also
reached from OUTSIDE that workspace — the metadata harness deps them through `packages/api/nros` — where
nothing enables it, so all 15 call sites failed at once and took `check-cli-tests`, and therefore tier 2,
down before `test-all`. A build that works only because of who else is in it. Fixed by switching both to
`wrap_err`/`wrap_err_with`. 13 more files still carry the compat import but are CLI-workspace-only today;
sweep + the Option caveat are recorded in the issue. See `archived/0521-*`. (2026-08-12)

RESOLVED 2026-08-11 — **#520** px4_msgs codegen raced ITSELF. `compile-check-fixtures.sh` runs once per
compile-check unit — 87 in parallel under `lane=all` — and every invocation regenerated px4_msgs into the
same three `<leaf>/generated` dirs. `stage_px4_msgs` stages through `<output>/.px4_msg_stage` and
`remove_dir_all`s it on every exit path, so concurrent runs deleted each other's staging mid-copy. It
surfaced as `Error: stage .../msg/GpsDump.msg / No such file or directory` naming a file that EXISTS —
15 distinct `.msg` names in one run, all 201 present, submodule clean at its pin — because the copy's
`wrap_err_with` prints the SOURCE while the ENOENT is the DESTINATION. Fixed with an advisory lock around
the codegen call only (the per-leaf `cargo check` is the long pole; locking the loop would serialize 87
units on nothing). Deeper fixes left open in the issue: a unique staging dir in `rosidl-bindgen`, and
generating once rather than 87 times. Third lane blocker in a row after #0500 and #0510.
See `archived/0520-*`. (2026-08-11)

independent causes. (A) `enforce_registry` flagged three scripts as compile-at-test; one COMPLIES —
`package_xml_comment_stripping.sh` runs `cmake -P`, CMake's INTERPRETER, and its header says "Buildless".
The detector matched the prefix `"cmake -"`; registering it would have laundered a non-violation into the
registry. `cmake_line_builds()` now reads the MODE. The half that took two iterations was not `-P`: a
"default true so unknown spellings fail closed" rule flagged two MORE compliant files, because `cmake`
appears here in prose (`echo "cmake output was:"`) and as another command's argument
(`nros_build_dir cmake workspace c`) — all pinned in a standing self-test, which also caught the first
predicate reading only up to the next `cmake ` so `cmake -E env … cmake -S . -B build` looked like script
mode. The two genuine configures are registered `tool: "cmake (configure only)"`. (B) Three
`lane_build_covers_run` cases were not hanging — they called `nros_lane_coords_file`, i.e.
`cargo run -q -p nros-tests --bin lane-coords`, and `lane-coords` is a bin of THAT crate, so any edit to it
recompiles the package before a byte is written; the 60 s per-test timeout did the rest. **Corrects this
issue's own provenance:** not phase-340 W3's env change — the trigger is "any nros-tests edit, then the
first run", and those edits were mine (#470). Fixed in TWO passes, and the first was HALF a fix: changing only the test helper passed SOLO (9/9 in 0.5 s) and I reported it fixed on that evidence, but the next full sweep timed out on the same three cases — the GUARD UNDER TEST reaches the compile too, via `nros_fixtures_stamp_require` → `nros_lane_coords_file` → `cargo run`, which is instant solo and blocks past 60 s under a sweep's cargo-lock contention. **A solo pass cannot tell a fixed path from an unfixed one**, which is the trap this issue is about, walked into while fixing it. Pass 2 moved it into `nros_lane_coords_file` so every caller is covered; it prefers the NEWEST prebuilt selector and falls back to `cargo run` when any `nros-tests` source is newer. Settled by a full sweep: TIMEOUT 3 → 0, the suite absent from the failure list, real failures 14 → 10. Selecting it by preferred profile picked an 11-day-old artifact answering 12 coordinates
where the sources say 13 — now newest-by-mtime; a staleness check written for that was REMOVED as
unreachable rather than shipped with a confident comment. See `archived/0523-*`. (2026-08-12)

Recently resolved (2026-08-11): **#513** (build) — `check-artifact-identity-budget` failed any
INCREMENTAL fixture build. The 0499 era filter counts only rlibs written since `started_at`, which
correctly excludes accumulation and also excludes everything cargo did not have to rebuild; a run whose
diff never reaches `nros_core` left ZERO of its artifacts in the window, so the "NONE for the budgeted
crate" arm — written for a partial build or a renamed crate — hard-failed a complete tree (`counted 16 of
244`, with four `nros_core` rlibs sitting there from 50 minutes earlier). Being FIRST in `check-fast`, it
stopped `ci` before the build tier, clippy and `test-all` — the exact harm 0499 was filed about. Fixed by
falling back to the whole tree when the window says nothing about the budgeted crate, labelled
possibly-historic. Chosen over a `.fingerprint/` liveness test because the fallback can only count MORE,
never fewer, so it cannot produce a false green — verified by forcing budget and ceiling to 1 and watching
it still FAIL. 0499's other two behaviours re-verified intact (all-history → SKIP; crate present → strict
filtered count). The advice was wrong too: the `else` arm claimed "this stamp has no started_at" when the
stamp had one, sending the reader after a missing file that was right there. Self-tested on every run like
the collation counter beside it, because the bug is INVISIBLE in output — a confident, specific, wrong
`NONE for nros_core`. See `archived/0513-*`. (2026-08-11)

RESOLVED 2026-08-12 — **#526** — the `trigger-test` feature DOES NOT LINK (six `undefined symbol: nros_platform_*` refs from
`nros-node`'s wake-latency-probe path), so every `#![cfg(feature = "trigger-test")]` test file is
uncompilable and lists ZERO tests. One of them is `wake_latency_cortex_m3` — the CI gate issue 0317
asked for — which has therefore been reporting nothing rather than failing. Found while fixing issue
0488's residue 1, which uncovered a SECOND defect in the same file behind it: `bench_image()` spelled
`target/.../release/` while the build writes the FreeRTOS carve-out `nros-minsizerel`, so even a linking
build would have taken the `[SKIPPED]` branch. That half is fixed; this one needs whoever owns the
feature's dependency set to provide the host platform symbols (the `posix-c-port` trick
`metadata_build.rs` uses for the same ABI). See `archived/0526-*`. (2026-08-12)

RESOLVED 2026-08-13: **#552** — the Zephyr Cortex-M (`mps2_an385`) C and C++ zenoh images died ~75 ms after
net init with `USAGE FAULT / Illegal use of the EPSR` and `PC = 0x00000000`, every register zero. NOT a NULL
function pointer, which is what that dump reads like and what this issue first claimed: it is a main-thread
STACK OVERFLOW. The C/C++ entry puts the executor's inline storage on the main stack —
`NROS_EXECUTOR_SIZE = 88192` against `CONFIG_MAIN_STACK_SIZE = 16384`, a 5.4x overflow — so the SP walked
into `z_idle_threads` and `g_sessions` and the CPU stacked an all-zero exception frame. RUST passes on the
same board because its entry does not allocate there, and native_sim's main thread has no 16 KB stack; the
language split that looked like a registration-seam clue was the allocation site. Same class as FreeRTOS's
64 KB `APP_TASK_STACK`. Attributed by dumping the frame under gdb with a router on the image's BAKED port
(`tcp/10.0.2.2:10700` from `port_of` — without a listener the image exits via `rc=-100` and never faults),
then confirmed in one line by `CONFIG_HW_STACK_PROTECTION=y`: `FATAL ERROR 2: Stack overflow ... thread:
main`. Fixed by raising the stack to 131072 and KEEPING the MPU guard on, so the next overflow names itself.
Both cells pass, zero faults. Follow-up worth filing: `NROS_EXECUTOR_SIZE` is known at build time and no
board conf is checked against it. See `archived/0552-*`. (2026-08-13)

RESOLVED 2026-08-13: **#544** — every Zephyr RUST fixture leaf failed at `cargo build` with `the argument
'--no-default-features' cannot be used multiple times`, taking the zephyr module — and `ci-matrix` — down,
while the C/C++ lanes stayed green because they never see `EXTRA_CARGO_ARGS`. Patch-site drift:
`cargo-features-patch.sh` injects the pass-through INSIDE `add_cargo_target_with_zephyr_env` (covering every
caller), but its comment described a pre-refactor upstream layout — "two such lines: cargo build (~199) and
cargo doc (~243)" — so a reader concluded it was half-applied and hand-added copies at BOTH call sites, which
its own marker-grep could not see. Function-level plus caller-level = the flag twice. Fixed by making the
script REPAIR as well as apply: it strips caller-level copies (upstream's lines are bare, so this restores
upstream text exactly), corrects the comment, and now FAILS unless the variable appears exactly once in code
— comments excluded, since both sites carry prose naming it. Verified on all four module states: repaired,
drifted, repeat, and pristine `404fcef`. The `patches.yml` delivery is deliberately NOT added: a second
delivery path for the same injection is what caused this, and there is no BYO 4.x workspace here to test
`west patch` against — the exactly-once guard is the precondition that makes adding it safe later. See
`archived/0544-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#542** — the C/C++ metadata probe writes `set(NROS_EXTRA_CPP_FEATURES "metadata-mode")`, and
`nros_feature_set()` appends that to WHATEVER crate it assembles — so the probe asks `nros-c` for a
feature only `nros-cpp` has (`git log -S` shows nros-c never had it) and the build dies with
`the package 'nros-c' does not contain this feature`. C/C++ components therefore cannot regenerate their
sidecars: `nros sync examples/workspaces/safety` reports "no producer" for 3 of 4. Invisible on a warm
checkout because the sidecars are gitignored and already present, and no lane deletes one — the same
"path nothing executes" shape as 0488 residue 4. It also explains 0522's 50.26 GiB: those 14 probe trees
are residue of builds that compile the runtime and then fail, so 0522's keep-or-delete measurement is
blocked until this is fixed. Same hook as issue 0304, one level on: applying it in ONE place is not
enough when that place serves two crates. See `archived/0542-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#543** — the metadata probe builds a component WITHOUT the bringup's declared capabilities, so a
component using a capability-gated API cannot be probed: `nros sync examples/workspaces/safety` reports
`'class nros::Node' has no member named 'create_subscription_with_safety'`. Not API drift — the method
exists behind `#if defined(NANO_ROS_SAFETY_E2E)`, the workspace declares `features = ["safety"]`, and
`NanoRosCapabilities.cmake` lowers it; the probe's generated CMakeLists just carries no capability input
at all. Surfaced once issue 0542 was fixed (that workspace's failures went 3 → 2). The general rule it
breaks: a probe that compiles the user's source must build it with the user's configuration, or it is
answering a question about a different program. Fix should reuse the ONE lowering rather than re-deriving
it through the per-crate feature hooks (the phase-314 argument). See `archived/0543-*`. (2026-08-13)

RESOLVED 2026-08-13 (see the combined entry above) — **#548** — the XRCE C shim (`nros-rmw-xrce/src/{session,platform_aliases}.c`) linked against
`nros_platform_clock_{ms,us}`, which RFC-0073 / phase-352 replaced with `clock_ns` plus STATIC INLINE
wrappers — no port defines them any more. Every Zephyr XRCE leaf therefore fails at link with 5 undefined
references, and since the zephyr module is an order-only prerequisite of every platform it takes the
tier-2 fixture build down. The shim does `#include "nros/platform.h"`, so on the Zephyr path that include
is resolving to a stale copy still declaring them `extern`. Same family as the stale committed
`nros_generated.h` fixed in `5dc2fa869` the same day — the rename landed without every header CONSUMER
following it, each failing differently. Found while running tier 2 for issue 0528's acceptance: 0528's
own symptom is GONE (zero `EXECUTOR_OPAQUE_U64S` asserts from scratch, was six leaves) and the build now
reaches leaf 12. Probably migrate the callers to `clock_ns` — `uxr_nanos` currently scales microseconds
back up by 1000, losing precision the ns clock has. See `0548-*`. (2026-08-13)

RESOLVED 2026-08-15: **#574** — tier 2 and tier 3 demanded
`build/compile-check-fixtures/<id>/.inputsig` for the four WEST compile-checks and NO producer wrote it:
`compile-check-fixtures.sh` is the only `.inputsig` writer and its builder loop omits
`west-build`/`west-configure`, while `west-fixtures.sh` stamps `.compile-ok` under `build/west-fixtures/<id>/`
— different filename, different tree. A COMPLETE green `lane=tier2` was therefore followed by `ci-matrix`
dying at `_lane-gate` before one test ran, unfixable by rebuilding. Fixed in `72c297d36`: the west lane writes
the `.inputsig` too, hashing the WHOLE record line (a reconstructed 8-field prefix would read as PERMANENTLY
stale, worse than missing) and deriving the root with the same `nros_build_dir` call the probe makes, so
writer and reader cannot drift. `.compile-ok` stays — it records the BUILDER, which is what makes "configure
only" checkable. Verified at the GATE with the stamps deleted first: probe went from 4/4 missing to 4/4
`fresh`. Note for the next reader: the first post-fix run built only 1/4 and looked like a regression — the
in-tree CLI was stale after a 17-commit pull and these fixtures are a function of the codegen tool
(issue 0561 wearing another hat). See `archived/0574-*`. (2026-08-15)

Recently resolved (2026-08-14): **#561** — after `git submodule update` moved
`packages/cli/third-party/play_launch`, every `nros sync` failed the 0409 pin guard AND no sanctioned
command fixed it: `just setup-cli` reported success while rebuilding nothing, because it skips on
`nros source-stamp`, whose `is_cli_input()` excludes `/third-party/`. That exclusion is right for files
and wrong for ONE fact — `build.rs` bakes the submodule HEAD as `NROS_PLAY_LAUNCH_SHA`, so the pin IS a
build input. Fixed by folding the pin SHA (not the file list, so the "thousands of files" objection
stands) into `source_stamp()` AND routing `build.rs` through the same `play_launch_pin()`, so baked and
recomputed values are one expression instead of two that agreed by inspection; the 0419 walk-up gate moved
inside it. Regression test builds a REAL nested repo (a fake `.git` cannot reproduce the walk-up) and was
confirmed to fail without the fix. NOT swept: whether another `build.rs` in the closure bakes something
the stamp cannot see — third time this list has been found narrower than the build.
See `archived/0561-*`. (2026-08-14)

RESOLVED 2026-08-15 (not a live hole): **#578** — I split this out of #472's item 2 claiming that a `0` size
probe emits a `1`-wide opaque macro with only a build-script warning against linking it, and proposed 0360's
variant-symbol poison. Checked before implementing, and the premise fails three ways: (a) on probe-zero
`generate_config` returns BEFORE writing the per-build header, so a C/C++ consumer resolves the committed
stub and hits its `#error` at COMPILE time — earlier than the proposed link failure, and the `1` never
reaches a caller's `_opaque`; (b) `nros-c`'s `config` module and both executor asserts are `rmw-cffi`-gated,
and probe-zero IS the no-`rmw-cffi` case, so the value has no Rust consumer either; (c) the docstring's
fat-LTO-probes-zero case is historical — a release build with `lto = "fat"` + `rmw-cffi` now succeeds with
zero such warnings, so the poison would have added risk for no gain. What remains worth attention is a
DIFFERENT mechanism with an existing owner: the stub's `NROS_PLATFORM_NUTTX` arm includes a committed header
that bypasses the probe — the file #464 caught rotted ~11 % low. See `archived/0578-*`. (2026-08-15)

RESOLVED 2026-08-15: **#472** — thirteen of fifteen `*_OPAQUE_U64S` macros had no compile-time size check,
so a probe that under-stated a size was a SHORT BUFFER written past in C rather than a build error. The naive
fix is a no-op: the Rust consts are already `u64s_for::<T>()`, so asserting them against `size_of` is
tautological. The real exposure is that the HEADER's widths are probe-derived in `nros-build-helpers::c` —
two derivations of one fact — so the guards now compare each type against the number the header states, with
the probe-derived widths plumbed into the Rust config (the executor pattern, generalised to nine more) and
the config emitted AFTER the probes, which is the mechanical reason only the executor could be guarded
before. Tripwired: stating SESSION as 1 u64 fails the build naming the macro. Gated by
`check-opaque-storage-guards` in `check-fast` — whose FIRST version was false coverage and whose own
tripwire caught it (every macro is also a `pub const`, so "name appears in file" always said yes; it now
matches guard CONSTRUCTS). Corrections: the list of fifteen was stale (three CPP_* removed in phase 87.6/
87.11), and the five `NROS_CPP_RAW_*` are the same probe and types as their C counterparts. Item 2 ("probed
zero" poisoning the artifact at link) deliberately NOT done — split out as **#578**. See `archived/0472-*`.
(2026-08-15)

RESOLVED 2026-08-15: **#581** — `just book` had been broken on main, and each failure hid the next.
(1) `--features rmw-zenoh` names a feature RFC-0054 retired when the backends moved behind the CFFI seam
(`nros` has `rmw-cffi`, `rmw-cyclonedds`, `rmw-lending`), so cargo failed BEFORE rustdoc — and since
`cargo doc` is the recipe's first step, `mdbook build` never ran either: a book-only change could not be
previewed at all. (2) With that fixed, rustdoc surfaced EIGHT unresolved intra-doc links — one RFC-0073
fallout (`PlatformClock::clock_ms`, retired for `clock_ns`), five unqualified (`Node`, `SchedClass::*` are
not in scope in `node_runtime.rs`), two behind `safety-e2e` (correct links, feature not enabled — fixed by
enabling it, which also documents the safety API rather than deleting the links). (3) Then a public doc
linking a private `register_node_borrowed`. (4) Then `doc-rmw-cffi` pointing at `packages/rmw/cffi`, which
has no Doxyfile: phase-321 W2.e moved the SHIM crates there while the ABI crate and its Doxyfile stayed in
`packages/core/nros-rmw-abi`. All four doxygen recipes checked; the other three resolve. Verified by output,
not exit code: `nros/struct.Executor.html` exists, which is the link the recipe exists to keep from 404ing.
Root cause of the rot: no lane runs `just book` — wiring it into one is the durable fix and is NOT done.
See `archived/0581-*`. (2026-08-15)

**#524** — `anyhow` is unmaintained and this tree standardises on `eyre`. Census of every tracked
manifest and lockfile: the two FIRST-PARTY deps were both DEAD — `nros-build-profile` declared
`anyhow = "1"` with zero uses, and `packages/cli`'s `[workspace.dependencies]` entry was inherited by
no member — so both were deleted rather than ported (root lock diff: one line, the dependency edge).
What remains is transitive, in two chains: `play_launch_parser -> anyhow`, which is a FORK WE PIN and
therefore actionable via the vendored-fork workflow, and `wasip2`/`wasip3` -> `wit-bindgen` -> … ->
`anyhow`, which is upstream wasi tooling nothing here chooses. See `0524-*`. (2026-08-12)

RESOLVED 2026-08-13: **#534** — the Zephyr C zenoh leaves failed on `zenoh-pico/system/platform/zephyr.h:18:
fatal error: version.h: No such file or directory`, taking the zephyr fixture module — and with it `ci-matrix`
— down. ATTRIBUTED AT HUNK LEVEL, not by bisect: `292547dd5` (#529) added `zephyr` to `zpico-sys`'s platform
selection, arguing it inert because `build_c_shim` is skipped on Zephyr. Neutralising exactly that branch made
the leaf build; restoring it failed. The MECHANISM was one call above the shim, and my first reading of it was
wrong: `platform_name` also gates `build_zenoh_pico_unified`, so naming the platform made a BUILD SCRIPT
cc-compile the vendored `system/zephyr/*.c`, which need a `version.h` only Zephyr's own build generates. Fixed
by making the manifest's own comment ("no cc-rs consumer hits it") into a checked field: `compiled_by =
"platform"` in `[build.zenoh]`, an `Option<CompiledBy>` so an unset child cannot downgrade a parent. #529's
totality is untouched — the block is still resolved for every platform, only the cc build is gated. Verified
both directions by tripwire, and on the failing leaf (332 s, `zephyr.elf` linked, 0 errors). See
`archived/0534-*`. (2026-08-13)

RESOLVED 2026-08-13 (see the combined entry above) — **#528** — every Zephyr fixture leaf failed `EXECUTOR_OPAQUE_U64S too small for Executor + backing` in
`nros-c`'s compile-time assert, taking the zephyr module — and with it `ci-matrix` — down. CAUSE FOUND and
FIX LANDED 2026-08-13 (`e5bda71fb`): the shared `build/sizes-probe` dir was keyed `(rustc, target, features)`
while the sizing KNOBS (`NROS_EXECUTOR_MAX_CBS` and friends, resolved from env or Zephyr's `$DOTCONFIG` since
0460) change the probed SIZE and were not in the key — so a knob-16 leaf and a knob-default leaf shared one
probe dir and first-writer-wins. `probe_key()` now mixes `knob_identity()`; both probe orders verified.
Stays OPEN only until tier 2 completes end to end. Independently corroborated the same day by a parallel
sweep on a PRE-FIX tree, which hit it at `cpp-action-{client,server}-xrce` (a different coordinate than the
day before — the coordinate-hopping a first-writer-wins cache produces) and separated it from a sizing bug:
fails inside the 32-job sweep, ok when the build dir is wiped and rebuilt SOLO. That session also briefly
reopened this issue as "never fixed"; it had not pulled since 12:48, and the reopening is retracted in the
issue. See `0528-*`. (2026-08-13)

RESOLVED 2026-08-12: **#432** — `zephyr-lang-rust`'s DT codegen could not compile for ANY board with gpio
nodes, so Rust-on-Zephyr was native_sim-only. Fixed by phase-346 W2/W3, with the diagnosis CORRECTED: the
generator does not "drop a cell" — `arm,mps2-fpgaio-gpio` declares `#gpio-cells = <1>`, so the devicetree is
correct and `GpioPin::new` is what assumes two cells. Padding to the controller's count changes nothing
(measured); padding to the CONSTRUCTOR's arity is the fix, matching the C side's `DT_PHA_BY_IDX_OR(...,
flags, 0)`. The `gpio-keys` missing-`cfg:` half was as described. Delivered in-tree by
`scripts/zephyr/zephyr-lang-rust-gpio-patch.sh` and downstream by a `patches.yml` entry — the first
non-`zephyr` module there. Verified end to end: 4 x E0061 before, a real ARM ELF (`.text = 449876`) after,
built through the fixture path; the Cortex-M witness now builds `rust` beside `c`/`cpp`.
See `archived/0432-*`. (2026-08-12)

RESOLVED 2026-08-13 (see the entry above) — **#525** — one NuttX checkout serves both arches and NuttX built IN PLACE, so `.config` /
`include/nuttx/config.h` are last-configured-wins: which arch the tree holds is a property of BUILD ORDER, not
of the build being run. `lane=tier2` builds riscv after arm, and the state is STICKY — once a per-arch export
exists `build-nuttx.sh` skips reconfiguring ("export up-to-date"), so asking for ARM does not restore the ARM
config. This is what issue 0511 cost: the ARM image linked with the RISC-V memory map (`CONFIG_FLASH_SIZE=0` →
ROM LENGTH 0), read as a 400-500 KB size regression, survived clean rebuilds, and cost a retracted bisect. 0511
routed all four nano-ros build inputs onto the per-arch snapshot; the TREE is still last-configured-wins, so
nothing stops the next reader. Directions: make the provisioning script arch-idempotent, gate that no build
input names `$NUTTX_DIR/include`, or give each arch its own checkout. Not an argument against phase-337's board
consolidation — the arch IS discriminated via `CARGO_CFG_TARGET_ARCH`; phase-339 just migrated two of three
input classes. ALSO 2026-08-13 — a SECOND shared mutable tree with the same property: `nuttx-apps`. One symlinked
`apps/external/nano-ros` serves both arches, `Application.mk`'s `SUFFIX` is `$(CWD)` (identical for
both), `PREFIX` is empty so objects land beside their sources — including first-party
`packages/platform/**` — and the kernel `distclean` does not touch the apps tree. 0511's class, where
0511's fix and `check-nuttx-shared-tree-headers` do not reach (both key on `$NUTTX_DIR/include`).
Unreproduced; fix belongs to 0488 residue 4 with the ARCH in the coordinate. See `0525-*`. (2026-08-12)

RESOLVED 2026-08-13 — **#522** — the metadata probe built ONE FULL CARGO TREE PER COMPONENT (108 dirs, 82.4 GiB; 162 trees
holding 312 `libnros_core` rlibs with 16 distinct identities). **Cargo-harness half FIXED 2026-08-12:**
`metadata_build.rs` now resolves a shared target dir (`$NROS_BUILD_ROOT/metadata-probe`, else
`<nano-ros workspace>/build/metadata-probe`, else a `.shared-target` beside the harness dirs for a
read-only out-of-tree SDK). Measured on `examples/workspaces/rust`: 6 dirs / 3.2 GiB / 12 rlibs -> 1 dir
/ 483 MiB / 2 rlibs, and cold `lane=native` got FASTER (581 s -> 461 s steady state). Unique per-component
package + bin names were part of the fix, not tidy-up: cargo does not hash the final artifact name, so a
shared dir with one `probe` binary is phase-340 W1's last-writer-wins collision. **STILL OPEN** for the
second producer — the corrosion-driven `metadata-probe-cmake` path, 14 trees / 50.3 GiB, whose dir is
chosen by cmake and belongs with issue 0493. RE-CHECKED 2026-08-13: the cmake half is NOT the same defect — its location is already correct (per WORKSPACE, in the workspace's own `build/`), and 4.7 of its 4.8 GiB is CORROSION's cargo tree, so it is duplication (14 copies of one dep graph) rather than misplacement. Corrosion offers no target-dir knob; the only lever is the consuming project's `CMAKE_BINARY_DIR` — and 0.6.x now hashes by workspace manifest, which is exactly the collision 0493 recorded against `< 0.6.0`, so the pin bump this session may have unblocked sharing. Belongs with 0493. See `archived/0522-*`. (2026-08-12)

RESOLVED 2026-08-12: **#511** — `rust-rtos-link-check` "overflowed NuttX ROM by N bytes" because the ARM image
was linked with the RISC-V memory map, where ROM has LENGTH 0. N was never an excess — it was the image's whole
ROM-placed size against a zero-length region, which is why it stayed constant across revisions, survived clean
rebuilds (the stale `.config` lives in the submodule, not any target dir), and why no revision ever "fit".
phase-339 W2 moved the linker SCRIPT onto the per-arch export snapshot but left the cpp `-isystem` on the SHARED
tree, so `CONFIG_FLASH_SIZE`/`CONFIG_RAM_*` came from whichever arch was configured last — and `lane=tier2`
builds riscv after arm. Fixed by taking the headers from the same snapshot as the script, plus a
`rerun-if-changed` on both spellings of `nuttx/config.h` (0477's rule: the config IS the memory map). Verified:
the leaf that reported 424088 bytes of overflow now links at `.text = 421880` and the whole lane passes — the
tier-2 sweep's last red. My earlier bisect of this was retracted first; there was no regression to find.
See `archived/0511-*`. (2026-08-12)

Recently resolved (2026-08-13): **#512** (testing) — `check-readiness-marker-literals` was blind to the
WORST case. It flagged a literal that matches a constant exactly, or ambiguously prefixes two — but a
literal matching NOTHING was silent, and that is the only case GUARANTEED to fail: it never matches, so the
wait burns its whole timeout and the test blames the fixture. It had already cost 108 s of a 137.9 s suite
(issue 0489, `esp32_emulator.rs` waiting on `"Waiting for messages..."` after phase-342 W7 converged the
examples), with the gate reporting `OK (32 baselined, 0 new)` throughout. Fixed with a NARROW rule — flag a
literal that EXTENDS a known constant, i.e. opens with a marker this module defines and then pins more of it
than the constant guarantees. Not "any literal matching no constant", which this issue argued would fire on
every ad-hoc pattern and be switched off within a week: verified that `"crc=ok"`, `"data:"` and
`"Booting Zephyr OS build"` all pass untouched. Proven on one identical tree with 0489's literal
reintroduced — NEW gate errors naming `LISTENER_WAITING_BANNER`, OLD gate reports OK. Zero hits on the
current tree, so it lands green and fires only on the regression, which is why it needed no baseline. See
`archived/0512-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#530** `FREERTOS_PORT` is UPSTREAM's variable name with incompatible values: upstream takes an
ENUM keyed into a 1356-line generator-expression table (`GCC_ARM_CM4F`), `nros_freertos_build_kernel()` takes a PATH
FRAGMENT under `portable/` (`GCC/ARM_CM3`). A user arriving from upstream docs, any tutorial, or a Plus-TCP demo got a
missing-source error naming a `.c` file they never typed, with no mention of the variable. Now accepts EITHER spelling
(upstream's enum is translated — the compiler is the first underscore-separated token in every upstream port name — and
the translation is announced, not silent), and failing resolution errors AT the variable listing both forms plus the
compiler dirs actually present. Forward-compatible on purpose: phase-349 W3 retires this builder for upstream's own
CMakeLists, at which point the enum is the only vocabulary. See `0530-*`. (2026-08-13)

RESOLVED 2026-08-15: **#529** — the zpico resolver could never select `zephyr`, so
`config/zephyr/nros-platform.toml`'s `[knobs.zenoh.tx]` — the only such table in the tree — was unreachable
and editing it did nothing. Landed in `292547dd5` and verified 2026-08-15: the resolver is total
(`else if use_zephyr => Some("zephyr")`), and `check-zephyr-knob-agreement.py` is wired into `check-fast`
and passes, comparing the Kconfig defaults against the TOML so the two sources cannot drift silently — the
part this issue called the one with lasting value. NOTE the trap it predicted sprang immediately, mirrored:
naming the platform is ALSO what gates `build_zenoh_pico_unified`, so a build script started cc-compiling
the vendored `system/zephyr/*.c` and every Zephyr C zenoh leaf died on `version.h` — issue 0534, fixed by
`compiled_by = "platform"`. "No behaviour change today" was true of the knobs and false of the build, and
both were the same condition. See `archived/0529-*`. (2026-08-15)

RESOLVED 2026-08-11 — **#518** `SourceMetadata` could not be parsed AT ALL: `SourceTimer` is
`deny_unknown_fields`, and #505 *added* `period_us` beside the kept `period_ms`, so every sidecar parse died on
`unknown field 'period_us'` — two reds on `main` in `plan_pipeline_e2e`, reproduced on a clean tree. **This
issue's first draft misdiagnosed it** as a half-done rename and proposed renaming the reader's field; #505's
commit message says the change was deliberately additive ("`period_ms` kept for existing consumers"), and the
rename made the tests fail the other way (`unknown field 'period_ms'`, from a fixture that correctly still carries
it). Fix is additive: `period_us: Option<u64>` with `default`. Survived review because `SourceTimer` has no field
readers anywhere — only a test parsing a real sidecar can catch it. The general shape:
`deny_unknown_fields` turns a backward-compatible producer change into a hard failure in a consumer nobody thought
to look at. See `0518-*`. (2026-08-11)

RESOLVED 2026-08-11 — **#516** every regex reader of `package.xml` treated a **commented-out** element as a real
declaration. cmake has no XML parser, so all seven readers matched regexes against raw file text, and a regex
cannot tell an element from the same element quoted inside a comment. The `<depend>`-presence readers were the
likeliest to fire in the wild — commenting a dependency in and out is routine ROS practice, and
`NanoRosVerbs.cmake` used mere presence of a `<depend>` tag to decide whether to run interface codegen. Surfaced by
phase-348 W1: the first provider `package.xml` explains the provision-vs-consumption distinction in a comment that
quotes the other tag, and `nano_ros_read_package_export()` then reported that file as consuming `rmw=zenoh` — the
file was correct, the reader was not. Fixed with ONE shared helper (`nros_read_package_xml_body()`) across all
seven sites, not a strip where the symptom appeared. The pattern is `<!--([^-]|-[^-])*-->`, never `<!--.*-->`:
cmake regexes are greedy with no lazy quantifier, so the naive spelling silently deletes every element BETWEEN two
comments. Gated by `check-package-xml-comments`, whose three cases were each verified to fail under the matching
perturbation. See `0516-*`. (2026-08-11)

RESOLVED 2026-08-12 — **#517** the fixture resolver spelled a row's VARIANT identity as a leaf path
literal, so `target_dir` could not be deleted. The framing was wrong: all 124 cargo rows are SHARED, so every
one builds into `build/cargo-fixtures/<slug>` and none writes to its leaf `target*` at all — the leaf root was a
KEY encoded as a path, surviving only because `require_in_lane` re-derived the row FROM that path. Fixed by
making the lane check take the ROW (`require_coord_in_lane` + `select_row`), converting the ~17 inline spellers
(`select_sole_row` for the 83 one-row leaves, `FixtureVariant::rmw` for the rest), then deleting all 41 keys —
with the group slug byte-identical for all 124 rows before and after, so nothing moved on disk and no rebuild
was needed. Four invariants that encoded the old property were re-aimed rather than deleted, including
`check-fixture-groups`'s A2b arm, whose advice had been "give one row its own `target_dir`". Closes phase-340
W2.d. See `archived/0517-*`. (2026-08-12)

RESOLVED 2026-08-10 — **#510** the px4 companion lane skipped `nros sync`, so all three leaves
(`px4-stub`, `px4-probe`, `offboard-companion`) resolved their registry-named nros deps
(`nros = { version = "*" }`, `nros-platform-cffi`, `nros-rmw-xrce-cffi`) against the PUBLIC crates.io:
`error: no matching package named 'nros' found`. The recipe's comment — "no `nros sync` path — px4_msgs
isn't an ament package" — is true of the CODEGEN half and false of the other one: sync also writes the
leaf's `[patch.crates-io]`, which is gitignored and therefore absent in every clone. #378's class. It
takes `build-test-fixtures lane=all` down with rc=2, so no tier needing the full existence set can finish
— and it hid because `lane=tier2` reports the same three as a soft `cargo-check FAILED … (no stamp)` and
still exits 0. Fixed by syncing each leaf after codegen, before the cargo build. See `archived/0510-*`.
(2026-08-10)

**#532** (embedded, open 2026-08-13) — the platform clock ABI fixes a UNIT (`clock_ms` + `clock_us`) but
cannot express RESOLUTION, so every port either lies or truncates: microseconds are a lie on ThreadX
(100 Hz tick = 10 ms steps) and the mps2 bare-metal port (`clock_ms() * 1000`), while nanoseconds are
discarded on POSIX and on boards whose hardware resolves 40 ns (mps2 SysTick, measured) or 6 ns (STM32F4
DWT). The same header already advertises ns for the WALL clock while monotonic stops at us. Issues 0502,
0515 and 0531 are three symptoms of the one missing fact. Surveyed all six backends: a Linux-style
clock-id interface is un-implementable with fidelity on four of them (no ids at all on
FreeRTOS/ThreadX/ESP-IDF; NuttX's `clock_getres` returns the same tick number for every id), and
resolution is a per-board, sometimes per-BOOT value (Zephyr runtime cycle rate, ESP-IDF APB rescale), so a
compile-time constant cannot carry it. Proposes one `clock_ns` symbol plus `clock_resolution_ns`, with
`clock_ms`/`clock_us` as header wrappers. See `0532-*`.

RESOLVED 2026-08-13 — **#535** the 74 west fixtures are manifest rows (70 zephyr leaves + the four
`west-fixtures.sh` ones), the emitter reads them, and the lane narrows by coordinate: tier 2 builds 7
instead of 70, measured 592 s → 76 s. The two literal-path fixtures this issue also named are fixed by a
different mechanism than a row: neither IS a fixture in the manifest's sense — each is a postprocess of
another row's artifact — so what they shared with their consumers was a PATH, and the fix is the KIND.
`target-zenoh-fixture-posix/` moved out of the repo root to `build/zenoh-fixture-posix` (RFC-0070 R1) and
the esp32 `.bin` literals in seven places became `kind::ESP32_QEMU`. The filing's list was incomplete —
three tests read the zenoh fixture, not two; seven esp32 sites, not two — and one bug was caught before
landing: `just esp32 clean` had been routed onto the ARM zenoh-pico constant and would have deleted the
wrong tree. See `archived/0535-*`. (2026-08-13)

RESOLVED 2026-08-13 — **#536** the four `west-fixtures.sh` fixtures are `[[compile_check_fixture]]` rows with
two builders (`west-build`, `west-configure`), `output` as the stamp gate, and the builder recorded in the
stamp. Three build with `--cmake-only` now. The filing's cost claim did NOT survive measurement: "pay for a
full kernel build" is true of DISK and only for one fixture (93 MB → 7.3 MB); the self-pkg pair costs
nothing to stop because it fails at cmake GENERATE, before any compilation. See `archived/0536-*`.

RESOLVED 2026-08-13 — **#538** `fixture-inventory.py` is gated (`--check`) and its four stale rows are
deleted. The open half — whether the file survives at all — is decided: KEPT. Its hand-authored half is now
redundant and down to one true entry, but `prerequisite_rows()` models 22 SDK-prerequisite / preflight /
`shared_mutation` facts no manifest row carries, and phase-339 treated that model as an obligation ("a stale
`shared_mutation` is worse than none"). See `archived/0538-*`.

RESOLVED 2026-08-13 — **#540** `bins/int32-observer` deleted, and the CLASS it hid in is now enforced:
`fixture_source_coverage.rs` asserts every crate under `bins/` is a manifest row or a tracked exception,
failing in three directions (each verified red first). The two live bins this issue named as unrowed are
handled — `logging-smoke-zephyr-native-sim` has a west row (0549 removed its duplicate builder) and
`ros-edition-pose-pub` is an allowlisted RFC-0058 exception. See `archived/0540-*`.

RESOLVED 2026-08-15 — **#580** every cyclone interop test named its DDS domain with a LITERAL (117/118 in
the two shell scripts; 99 ×6, 88, 42 across the C++ binaries), so two concurrent runs joined one bus and
the collision presented as wrong DATA: a tier-2 sweep showed `nros sub captured 'hello-from-nros'` — case
A.1's own publisher payload — while solo runs passed 17/17 twice. Fixed with ONE scheme in three languages
(`ros2_e2e_common.sh`, `nros_test_domain.h`, both mirroring `nros_tests::unique_ros_domain_id`):
`ROS_DOMAIN_ID` when set, else a per-process domain in 1..=232. Note for the next person: two concurrent
copies of the FAILING TEST passed — the collision needs one suite's A.1 publisher alive during the other's
A.2 window, so only two concurrent full SUITES reproduce it. A failed repro at the wrong granularity is not
evidence of absence. And fixing only the shell half moved the failure to `service_roundtrip`, one file over.
See `archived/0580-*`.

RESOLVED 2026-08-15 — **#576** the four west-built compile-checks (`west_bringup_zephyr`,
`west_board_import`, `zephyr_self_pkg_rust`, `zephyr_self_pkg_sibling`) never wrote the `.inputsig` the
staleness gate reads, so tier 2 and tier 3 died at the gate right after a fully green
`build-test-fixtures lane=all`. Build side wrote `.compile-ok` under `build/west-fixtures/<id>/`; the gate
read `.inputsig` under `build/compile-check-fixtures/<id>/` — the issue-0196 rule, in the compile-check
inventory. #0554 dropped these rows for `SCOPE=native` and rightly KEPT them for `all`/`coords`; this was
its missing half, since keeping the demand is only sound if the building lane writes what the gate reads.
`west-fixtures.sh` now stamps the sig on the same success condition, passing the RAW manifest line —
the signature hashes the record verbatim, so an 8-field reconstruction of an 11-field line would have
turned "missing" into a permanent "stale". See `archived/0576-*`.

RESOLVED 2026-08-15 (phase-353) — **#509** the Zephyr lane was per-leaf-overhead bound, and every
direction is now closed or refuted. FIXED: sync restamping byte-identical files (#562), and
`west-fixtures.sh` wiping its build dir every invocation so a no-op replayed 1244 ninja edges — 1207 of
them C compiles — plus a 129-crate rebuild (phase-353 W2; no-op edges 1244 -> 0). REFUTED by measurement:
storage (iowait 0.25 % HDD vs 0.03 % NVMe, host since doubled to 125 GB RAM with the Zephyr build root
already on SSD) and concurrency (the `/8` divisor is inert under the fifo jobserver; box 76 % idle). The
title's numbers are all superseded — the 40 min was mid-sweep with eight families competing. Carry
forward: **wall-clock is not a usable instrument on this host** (50–695 s for provably identical work) —
count edges and leaves. Direction (3), fewer COLD leaves, continues as #604. See `archived/0509-*`.

**Re-measured 2026-08-13 (phase-350):** a fully warm lane is **592 s (9 m 52 s)**, not 40 min — the
original figure was taken MID-SWEEP with seven other families competing, so it should not be quoted as the
lane's standalone cost. The narrowing this issue asked for now exists (phase-350 W1.b): tier 2 builds 7
leaves instead of 70, **7.8×**, not the 10× the leaf count implies — per-leaf is 8.5 s over the full lane
vs 10.9 s over tier 2's seven, because lane-level fixed cost does not shrink with the leaf set. That
CONFIRMS this issue's core claim from the other direction. Its closing question ("can the cover retire
some leaves?") is answered NO in W4: the 26 leaves no lane selects sit on coordinates with Runtime cells.

**#507** (rmw, open 2026-08-10) — the cyclonedds fork carries TWO nano-ros-only lock changes
upstream lacks: striped addrset locks (`942dda3c`) and the Zephyr-native ddsrt sync backend
(`a09babf3`). Upstream `5e82de60` still has the per-addrset mutex and no Zephyr backend, so this
is a standing rebase cost, not a wait-for-release. Every rebase must re-establish that nothing new
holds two addrset locks or one across a callback (the striping makes either a deadlock;
`addrset_striped_lock_concurrency` is mutation-validated cover). The addrset half is the one worth
upstreaming — not Zephyr-specific, removes an allocation per addrset everywhere, and its two
nesting fixes are correctness wins. Needs a `WITH_ZEPHYR` option in cyclone's own ddsrt CMake,
which nano-ros never needed. See `0507-*`. (2026-08-10)

Recently resolved (2026-08-12): **#508** (rmw) — the freertos/threadx ddsrt sync ports `abort()`ed on
init failure with nothing logged. Fixed in `cyclonedds@8601ca66` (pushed, pin bumped): one helper per
port, naming the object and — on ThreadX, which has one — the `tx_*_create` status. The open question
this was filed on ("how to emit it without dragging logging into ddsrt's lowest layer") needed no
decision: both files already include `dds/ddsrt/log.h` and already use `DDS_FATAL` for the same class
of failure a few lines below, and `dds_log` aborts on `DDS_LC_FATAL` outside any level filter, so the
unconditional abort survives. Build coverage is asymmetric and recorded in the issue: nothing here
configures `WITH_FREERTOS`, so that TU was checked with `-fsyntax-only -Wall -Wextra`. See
`archived/0508-*`. (2026-08-12)

Recently resolved (2026-08-10): **#501** (testing) — `native_main_macro_misuse` failed a DIFFERENT subset
of its five cases every run; four of five failed at least once, any one alone passed. Not lock contention,
which was the first read — it fails SERIALLY too. All five staged copies build the same package name
`demo_entry` into one shared `CARGO_TARGET_DIR` (phase-342 W2), and a package is identified by
name + version, NOT by the path it was staged to, so the first case to compile it SUCCESSFULLY made every
later case's check fresh: `Finished` with no `Checking demo_entry` and exit 0, in a suite whose every
assertion is "this misuse must FAIL to compile". Confirmed deterministically OUTSIDE nextest (stage two
copies, check a valid one, then a misuse one against the same dir — the misuse exits 0). Fixed by stamping
a unique build-metadata version per staged copy, keeping the shared dependency graph that phase-342's
108.5 s -> 10.3 s actually came from. **The issue's claim that this explained #495 is RETRACTED** — 0495
reproduces cold and alone, and disabling either fix leaves the other's tests passing. Two independent
defects in one file with a similar-looking symptom; "same symptom, same file, must be the same bug" is what
made the first write-up assert an untested link. See `archived/0501-*`. (2026-08-10)

RESOLVED 2026-08-10 — **#500** `examples/workspaces/mixed` could not LINK: seven duplicate symbols
(`nros_rmw_zenoh_register`, `REGISTRY`, `nros_rmw_cffi_*`) from two `nros-rmw-zenoh` identities in one
`libnros_ws_runtime.a`. The cause was NOT the workspace — the build was using **Corrosion 0.5.1 on a host
where 0.6.1 was installed**, and `< 0.6.0` names the cargo target dir with a constant, so two workspace
roots share one `deps/` (#0493's topology finding). `_nros_corrosion_prefixes` globbed the SDK store and
`find_package` took the first prefix that resolved, so a months-old `0.5.1-nros1` outranked the
`0.6.1-nros1` a provisioning run had just written: `just workspace install-corrosion` and `nros setup
--tool corrosion` both printed success and changed nothing. Fixed by ordering the store newest-first in
BOTH derivations (cmake `COMPARE NATURAL ORDER DESCENDING`, shell `sort -Vr`), asserted by
`check-cmake-corrosion-prefix` and mutation-tested. Only #0493's resolution-reporting line made this
visible; without it the retry would have been logged as "rebuilt on v0.6.1, still broken". Verified with
both versions present: resolves 0.6.1, exit 0, 0 duplicate symbols. See `archived/0500-*`. (2026-08-10)

Recently resolved (2026-08-11): **#514** (embedded) — every runtime contract rule pushed verdicts into a
ring nothing read: `drain_violations` and `nros_diagnostics::Reporter::report` each had ONE caller in the
tree and it was a test binary, so on a real image a continuously-violated contract and a met one produced
identical output (none). Fixed by logging each violation AT DETECTION plus a `violations_dropped` counter
for what the bounded ring could not hold; `set_report_violations(false)` opts out. Logging at detection
rather than draining at end-of-spin is the load-bearing choice — the first attempt drained-and-cleared and
broke four spin-then-drain tests, the same regression a user with a custom reporting path would have hit.
Verified on the FreeRTOS lane: 67 violation lines under a 2 kHz flood, none idle. `/diagnostics`
publication and non-placeholder `fqn`s remain open. See `archived/0514-*`. (2026-08-11)

Recently resolved (2026-08-11): **#515** (orchestration) — a timer period that is not an integer multiple
of its tier's `spin_period_us` quantizes to the spin grid (measured: 33 ms on a 5 ms spin alternates
35 ms x475 / 30 ms x303, mean 33.001), and because the rate is preserved and nothing is dropped, every
runtime rule is correctly silent. The executor now audits its timers once, on the first spin carrying a
non-zero timeout, and logs the declared period, the spin period, and the two values activations will
alternate between — verified on the FreeRTOS lane, which emits exactly two warnings naming 30000/35000 us.
A RESOLVE-time diagnostic remains the better version and is not what landed; the runtime backstop is
complementary (it also catches hand-written spin loops). See `archived/0515-*`. (2026-08-11)

**#506** (embedded, open 2026-08-10) — transport tasks ABOVE application tiers is the right FreeRTOS
default (the inverse starves the RX drain into multi-second lwIP RTO freezes, d708d8c5b), but the band has
NO BUDGET: sustained inbound above the ~750 msg/s drain capacity (mps2-an385/QEMU lane) triggers periodic
recovery cycles in which the transport band runs solid for ~100-340 ms and every application tier's timers
gap at the same instant — a single too-fast remote publisher can blow every deadline on the device, and no
tier priority can prevent it. Fix: budget the drain (sporadic-server-style), or cap per-subscription
inbound at the rmw ring (tail-drop beats protocol-recovery preemption), plus a shed counter. See `0506-*`.

Recently resolved (2026-08-15): **#505** — periodic timers replayed the whole backlog after a stall. The
code landed 2026-08-11 (`TimerOverrunPolicy::{Skip,CatchUp}`, Skip default, saturating `overruns`,
phase-preserving remainder, microseconds end to end, `timer-overrun-runtime`); what remained was phase-358
W2's other half, which code cannot satisfy — the policy WRITTEN DOWN. Now RFC-0002 § 4.4a + the book, with the
argument that justifies the default: under `CatchUp` a tier stalling up to 611 ms still reports 100.03 Hz on a
declared 100 Hz loop, so `rate-hierarchy-runtime` is structurally blind to the fault it exists to catch.
Verified against the code, not the issue's own summary. Out of scope: no diagnostics drain on the FreeRTOS
lane, policy not declarable in launch metadata, period/spin quantization silent. See `archived/0505-*`.
(2026-08-15)

**#0594** (build, open 2026-08-07) — **`alloc` and `std` are turned on implicitly in 34 places, and
picking a PLATFORM enables the heap.** `nros-c`/`nros-cpp`'s `platform-{zephyr,freertos,nuttx,threadx}`
and `nros-rmw-zenoh-staticlib`'s five platform features all list `"alloc"` (or `"std"`); nros-cpp's
manifest states the reason — *"Embedded platforms imply `alloc` so the C++ FFI layer's `extern crate
alloc` compiles"* — i.e. a nano-ros internal compile is paid for out of the user's image. Nine
capability features (`param-services`, `lifecycle-services`, `bridge`, `config`, `metadata-mode`,
`signal-fd-wake`, …) enable what they require instead of requiring it. `global-allocator = ["alloc"]`
in three crates is gratuitous — all three allocator modules use only `core::alloc::GlobalAlloc`.
Separately, **`#[panic_handler]` was gated on the ALLOCATOR feature**, so "I need panic" had to be
spelled `global-allocator` — which is exactly how `compile-check-fixtures.sh:490` died on
`#[panic_handler] function required` under phase-361 W3, inside a `|| echo` that swallowed it.
**All 34 sites are now 0** and W8.c cut the
`#[global_allocator]` count **4 → 1**: `nros-c` and `nros-platform` had defined one under
IDENTICAL gates (kept apart by a manifest comment, while `nros-c` deps `nros-platform`
non-optionally), and `nros-platform-mps2-an385` / `zpico-alloc` shipped two more that bypassed
`nros_platform_alloc`. `nros-platform` is the sole owner; the rest forward to it, so a second cannot
be spelled. Two things fell out: `extern crate nros_platform` is load-bearing (an unreferenced dep
is DCE'd before its lang item lands — the FORCE_LINK class), and over-aligned requests now return
null instead of silently under-aligned memory. **Left open**: nothing GATES any of this — W4's
`check-feature-contract.sh` is still unwritten, so every figure here is a measurement, not an
invariant. See `0594-*` and phase-361 W2/W4/W8. (2026-08-07, W8.c 2026-08-10)
  **Retargeted at phase-359 W10 (2026-08-16):** 34 sites are down to **1** — `nros-tests`' `trigger-test = [… "nros-node/std"]`, a `std` forward W10 deletes. That site was INVISIBLE until today: clause (a) rejected `["std"]` in a feature body but not `["dep/std"]`, so the residual read as 1-and-benign with nothing having ruled on it. The clause now checks both spellings, carving out crates with no `no_std` mode (a hosted crate has no embedded image to protect — the same exemption clause (b) already makes). This one does NOT close with #0591/#0598: those are about `std` and W10 makes them unstateable, while this is about `alloc`, which survives as the remaining axis.
Recently resolved (2026-08-07): **#0593** — `nros/ffi-size-markers`, the `#[used]` attribute that
stops `--gc-sections` dropping the `__NROS_SIZE_*` statics the C/C++ opaque-storage macros are
probed from, was enabled by **exactly one thing**: `nros`'s `default` set. Both consumers dep
`nros` with `default-features = false`, so `cargo tree -p nros-c` resolved `nros v0.5.0 alloc,std`
— no markers. They appeared only in a whole-workspace build, by feature unification from an
unrelated member, while `nros-c`'s own manifest says the C ABI surface is "built per-platform by
cmake not by the workspace lane". Upstream of issue 0464's fallback chain. Resolved in phase-361
W3 — requested explicitly at all four dep-sites. **Not verified**: that it ever produced a wrong
macro value in a shipped artifact; `#[used]` acts at link time and this host cannot build the
C/C++ lanes. Summary in `docs/issues/archived/0593-*`.

Recently resolved (2026-08-16, phase-361 W3): **#0591** — `default = ["std"]` on the `no_std` crates made a
second compile identity out of an inert string; 19 crates split that way, ~8 s of 113 s redundant. Measured
today: **zero crates carry `std` or `alloc` in a `default`**, held by `check-feature-contract` clause (b).
**Do not reopen this on `cargo tree -d`** — it still reports `nros-core`/`nros-rmw`/`nros-serdes` twice and
that is the resolver-v2 HOST graph against the TARGET graph (`nros-orchestration-ir` side vs `nros` side),
which W3 recorded as legitimate when it measured that `default = []` merged no units at all. phase-359 W10
removes the `std` feature entirely, after which the narrow form is unstateable. See `archived/0591-*`.

Recently resolved (2026-08-16, phase-361 W7): **#0592** — a firmware build of `nros` compiled 58 crates,
39 of them only to run the `nros::main!` proc-macro (`syn`, `toml_edit`, `serde_yaml_ng`, the
`ros-launch-manifest` git deps, a duplicate `thiserror` major). `nros-macros` is now optional behind a
`macros` feature: **58 -> 19 crates**. The removal ORDER this issue proposed did not survive re-derivation —
the `model =` arm's 7 crates are on the MAINLINE `launch =` path (not the deprecated override), and `toml`
0.8 -> 0.9 turned out to COST a crate rather than save five (0.9 renames the stack: `toml_edit` ->
`toml_parser`, `winnow 0.7` -> `winnow 1.0`), with the un-split prize unreachable because `tokei` — a dev-dep,
already at its newest release — pins 0.8 and drags the old chain along. So the one viable step took the whole
prize. Its stated shape was illegal too: a default-on `macros` is unreachable when all 62
in-workspace dep-sites pass `default-features = false`, which `check-feature-contract` clause (d) rejects as
#0593's shape. Landed opt-in: 145 in-tree crates gained it, and the 52 that dep `nros` without invoking a
macro now stop compiling the subtree. Breaking out-of-tree — add `features = ["macros"]`. Lock did not move.
See `archived/0592-*`. (2026-08-16)

**#0596** (build, open 2026-08-15) — the `nros-launch-resolve` skew warning compares BINARY mtimes
(`[ "$_cli_bin" -nt "$_resolver" ]`), and `just setup-launch-resolve` is a no-op when cargo has nothing to
rebuild — so it never relinks, the mtime never moves, and the remedy the warning prints cannot clear it.
Fires on every `check-tier-preconditions` after any `setup-cli`, in the list that exists to name real
unmet preconditions (#0466). Also the wrong question: touching the binary would silence it while proving
nothing, and a real skew with a newer binary goes undetected. The hazard is genuine (#0363 C — the two
must agree on an argument list); the test for it is not. TWO spellings, `scripts/check-tier-preconditions.sh:145`
and `justfile:3958`, which must move together. Fix: give the resolver a SOURCE STAMP like the CLI's and
compare stamps. See `0596-*`. (2026-08-15)

**#0600** (build, open 2026-08-15) — `check-submodule-pinned-locks` reported "the submodule pointer moved
and the lock did not follow (issue 0560)" for a lock that is byte-identical to main's and names the crate
it supposedly lost: the real error is `failed to download hermit-abi v0.5.2 … --offline was specified`, i.e.
a COLD CACHE on this host. `cargo fetch --locked` pulled it plus six platform-irrelevant siblings and the
gate passed with the lock untouched — which is why `setup-launch-resolve` had BUILT the binary fine minutes
earlier (the build never needs them, only whole-graph resolution does). The misdiagnosis is not cosmetic:
it prescribes `just lock-update`, i.e. re-resolving a correct lock, which is exactly the churn #0359/#0378
were about. Match on the `--offline` marker and print `cargo fetch --locked` instead. See `0600-*`. (2026-08-15)

**#0599** (testing, open 2026-08-15) — `just/zephyr-ci.just:32` prints `Zephyr skip: zephyr-workspace not
set up` and then `exit 0`, so the driver records `== zephyr == OK` for a lane that built NOTHING. Twenty
minutes later `_lane-gate` fails naming four missing `.inputsig` files (`west_bringup_zephyr`,
`west_board_import`, `zephyr_self_pkg_{rust,sibling}`) — west-owned by design (`fixtures.toml:4553`) and
never skippable by lane narrowing, so every run scope requires them — and its remedy is
`just build-test-fixtures`, the command that just "succeeded". `check-tier-preconditions` does not mention
it either. The unprovisioned host is legitimate; reporting the skip as OK is not. Needs a third lane
verdict (SKIPPED), and both `exit 0` sites move together. #0196 shape. See `0599-*`. (2026-08-15)

Recently resolved (2026-08-16, phase-361 W2.a): **#0598** — `std` implied `alloc` in four crates and not in
five others, and `nros-core`'s SOURCE assumed the implication its own manifest did not make, so at the default
feature set `heap::Vec<u32>` existed with no `Serialize` impl. Measured today: **13 crates declare both, 13 of
13 carry `std = ["alloc", …]`**, and zero `any(feature = "alloc", feature = "std")` spellings remain — clause
(a/manifest) holds the edge, clause (a/source) rejects the respelling W2.a tried and reverted. phase-359 W10
deletes the `std` feature outright, after which nothing can imply anything. See `archived/0598-*`.

Recently resolved (2026-08-11): the on-target-time trio (embedded/api), filed from external RT-cadence
measurement and fixed as one series. #502: `nros_platform_clock_us` was MILLISECOND-quantized under a us
signature on FreeRTOS/ThreadX (a 1 ms error floor on every executor timer measurement); the FreeRTOS
Cortex-M port now interpolates the SysTick down-counter (tick-boundary + PENDSTSET races handled, runtime
LOAD read so tickless/non-SysTick ticks fall back; opt-out `NROS_PLATFORM_FREERTOS_NO_SUBTICK`), while
ThreadX stays coarse and now says so at the implementation. #503: `Record.timestamp_ns` was hardcoded `0`
everywhere; new opt-in `nros-log/platform-clock` populates it from the platform clock and `PlatformSink`
prefixes lines with `[sssss.uuuuuu]` (message rewrite — the `log_write` ABI has no timestamp param;
opt-in because the extern is a link requirement host tools without a platform port cannot meet). #504:
portable node code had NO clock at all; new `nros::time::now()/now_us()` mirrors the executor's source
selection (std `Instant`; no_std+rmw-cffi platform clock; compiled out elsewhere),
monotonic-since-unspecified-epoch, explicitly not ROS time. See `archived/0502-*..0504-*`.

Recently resolved (2026-08-11, phase-340): **#0499** — `check-artifact-identity-budget` counted every rlib in a long-lived tree, so accumulation and a real regression printed the same message, and the remedy ("delete the tree and rebuild") both cost a 40-minute wipe at tier-1 cadence AND erased the evidence of a genuine red. Fixed on all three axes: the stamp gained a `started_at` lower bound and the gate filters on it, `build-test-fixtures` reports the reading where the tree is known-fresh, and a failure now states whether accumulation is ruled out instead of asking the reader to guess. See `archived/0499-*`.

Recently resolved (2026-08-10): **#498** (build) — `just build-test-fixtures lane=native` died `metadata
harness emitted invalid JSON … EOF while parsing a value at line 1 column 0` — an EMPTY file, which was
1345 bytes and valid when inspected seconds later, and a straight re-run passed. `fs::write` truncates to
zero and then fills; `service-client` has three fixture rows (zenoh/xrce/cyclonedds) that run concurrently
and each run `nros sync` over the same leaf, so phase-340's per-RMW `--target-dir` isolation does NOT reach
a file keyed by COMPONENT. **The helper already existed, which is the actual lesson:** `cmd/ws.rs` had a
private `atomic_write` whose own doc called it "the write discipline every other sync-owned file here
uses", and the sidecar one directory over had three plain `fs::write` writers — a discipline inside one
file's private helper is a habit, and the sibling site is what a habit does not reach. Fixed as ONE public
`nros_cli_core::atomic_file` (private duplicate deleted), six writers converted — including
`mark_unprobeable`, which was NOT in the report and fails SILENTLY (a truncated marker reads "not
unprobeable" and pays the full failing probe it exists to skip). The generated harness inlines temp+rename
since it cannot depend on the CLI; that code is a string template nothing type-checks until a fixture
build, so it was verified by rendering it and compiling the emitted text. Gated by
`check-atomic-sync-writes`, three arms each verified to fail first — including a second `fn atomic_write`
appearing anywhere, which is exactly how the first helper failed to spread. **Same class as #494**
(`f6290fbdb`) — that one was caught because the output was wrong, this one because the read failed loudly.
See `archived/0498-*`. (2026-08-10)

Recently resolved (2026-08-10): **#0496** — cyclone called `ddsrt_mutex_init` per
**addrset**, and on Zephyr that is a slot from the static `CONFIG_MAX_PTHREAD_MUTEX_COUNT` pool,
so **joinable g. See `archived/0496-*`.

Recently resolved (2026-08-10): **#495** (testing) — `rebuilds_on_model_touch` failed: cargo
short-circuited in 0.04 s after the resolved model was touched. **Neither candidate the issue narrowed to,
and not 0490 unmasking it** — it reproduces cold, alone, with the shared target dir wiped. The macro READS
the model (`main_macro.rs:589`) and never registers it: `ensure_model` returned only
`[system.toml, launch file]` as rebuild deps, so `tracked.extend(inputs)` never saw the artifact — against
the module header's own stated invariant, "we emit `include_bytes!` for every file the macro read". The
confirmed `nros-build` edge was real and irrelevant: entry crates carry no `build.rs`, so it never runs for
this fixture. Fix is ASYMMETRIC and that is the point — the BUILD-PRODUCED branch now tracks the artifact,
the SELF-RESOLVED branch still must not, because that branch WRITES the file and depending on your own
output is the perpetual-dirty loop its `is_fresh` check exists to prevent. Only a writer can loop on its
own output. Matters past the test: a build-produced model changes without its inputs changing (`nros sync`
on a newer CLI, an expert `MODEL` override), and a consumer tracking only inputs is then a museum binary.
**Not the same bug as #501**, though they shared a file and a `Finished in 0.0Ns` symptom — each fix was
reverted independently to prove the other did not mask it. See `archived/0495-*`. (2026-08-10)

RESOLVED 2026-08-16 (phase-354 W1) — **#493** two cargo workspace ROOTS shared one corrosion target dir, so
`libnros_ws_runtime.a` bundled two `-C metadata` identities of ten crates and every `#[no_mangle]` export
collided. FIXED by the Corrosion `0.5.1 -> 0.6.1` bump (per-workspace hashed dirs), verified end-to-end
2026-08-10 and again 2026-08-16. Its long-open ENFORCEMENT item is now closed in both lanes: a
configure-time FATAL_ERROR below 0.6.0 here, and #0616's FATAL_ERROR when two roots claim one `--target-dir`
in the Zephyr lane. That gate found FOUR trees still silently on the 0.5.1 topology six days after the fix —
`Corrosion_DIR` is CACHED, so provisioning never touches an already-configured tree. Answers phase-354 W1's
either/or: the class is one artifact directory serving two workspace ROOTS; Corrosion `< 0.6.0` is one way
to arrange it, not the cause — the Zephyr instance had no Corrosion in it at all. Carried forward: whether
it reproduces in distrobox/CI is still reasoning, but a CI host on `< 0.6.0` now fails at CONFIGURE naming
the version. See `archived/0493-*`.

Recently resolved (2026-08-11, phase-347): **#492** — the CMake self-provisioned CycloneDDS built **slim GCC-LTO objects** and `ld.lld` cannot read GCC LTO
IR, so `build-test-fixtures lane=native` dies with 36 `undefined symbol: dds_*` while every obvious check says the
link is fine: `libddsc.a` IS in the whole-archive group, `nm` reports `T dds_get_guid`, and `-Wl,-t` shows all 148
members loaded including the definer. `readelf` finds no such symbol — it lives in GCC IR, which `nm` and `ld.bfd`
read via `liblto_plugin.so` and lld cannot. Minimal case: one object, `-fuse-ld=bfd` links, `-fuse-ld=lld` does
not. `nros setup --tool cyclonedds` is inert because phase 186 sets `CMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON`.
**The Rust self-provision has always set `ENABLE_LTO=OFF`** for exactly this reason, naming "same hazard on
native" — the CMake path, which every C/C++ example takes, never got it. One of two sibling paths fixed; the
third instance of that class this session. Fixed with `ENABLE_LTO OFF` on the self-provision, and re-proved twice since — phase-347 W5 rebuilt the cyclone fixture from scratch (13 MB binary, 28 descriptor symbols) and the post-corrosion-bump workspace rebuild completed RC=0. See `archived/0492-*`. (2026-08-10)

Recently resolved (2026-08-10): **#494** (testing) — `just ci-matrix` was NON-DETERMINISTIC: same tree, same commit,
**223 real failures then 20** on an immediate re-run, 203 of the first run's being
`lane-coords-tier2.txt: no coordinates`. `nros_lane_coords_file` used `cargo run … > "$out"`, and the shell
truncates the target the instant the redirection is set up while cargo compiles for seconds-to-minutes — so
every reader in that window saw a zero-length file. Blast radius came from a CORRECT decision: 0482's
narrowing fails closed on empty coordinates (an empty file must not read as "no narrowing"), so one
truncated file failed every narrowed test at once. Fixed by writing to a temp file and `mv`-ing into place
(atomic `rename(2)`). The determinism is the point — a gate that reports 223 then 20 teaches people to
re-run instead of read. See `archived/0494-*`. (2026-08-10)

Recently resolved (2026-08-10, phase-340): **#489** — every ESP32 test skipped `"qemu-system-riscv32 not
available"` on a host where `nros setup --tool esp32-qemu` had JUST succeeded — the skip message named the command
that had already been run. `esp32.rs` spelled the binary by bare name at three sites, so it resolved through PATH
to the SYSTEM qemu, which has no `esp32c3` machine, which is exactly what the probe rejects. `activate.sh`
deliberately keeps qemu OFF the global PATH (the `build/<tool>` convention), so PATH was never going to bridge it;
the arm family has had the resolver all along (`qemu_system_arm_path`) and ESP32 was written without it. Added the
twin — `QEMU_SYSTEM_RISCV32` → `nros_store_bin("esp32-qemu", …)` → PATH fallback. **A guard that names a remedy
should be tested against a host that has applied it.** Verified: `SUCCESS: ESP32-C3 QEMU boots and shows platform
banner`. See `archived/0489-*`. (2026-08-10)

Recently resolved (2026-08-10, phase-340): **#487** — `[system.*].check` allowed EXACTLY ONE probe, and libgcrypt
ships its two probes to two different distros: Arch's 1.12 has `libgcrypt.pc` and no `libgcrypt-config`, Ubuntu
22.04's 1.9 has the script and no `.pc`. So `cmd = "libgcrypt-config"` read a fully-installed libgcrypt as MISSING
and `nros setup --tool esp32-qemu` hard-failed telling the user to `sudo pacman -S libgcrypt` — a package
`pacman -Qo /usr/include/gcrypt.h` already reported as installed. There is no `--skip-system-check`, so a correct
host was simply blocked. Probes are now OR-ed (`at least one`, PRESENT if any finds it, MISSING only if a probe
ran and said no, UNKNOWN preserved for a probe that cannot answer — `sharedlib` off Linux, `pkg_config` with no
pkg-config). libgcrypt declares both. Negative direction verified: three genuinely-absent packages still report
MISSING, so OR-ing did not turn the gate green. See `archived/0487-*`. (2026-08-10)

Recently resolved (2026-08-10, phase-340): **#486** — `espflash` was documented in prose only, so a host that
completed `just esp32 setup` built every ELF and then produced NO flash images: the pack step warned and the lane
exited 0. Issue 0399's shape one dependency over (0399 declared `riscv-none-elf-gcc` because a
documented-provisioned host could not COMPILE; this is the same claim about the PACK step). Fixed in three parts,
because provisioning alone was not enough: `[tool.espflash]` pinned to v4.5.0 + added to the board's `packages`;
`activate.sh` puts the store bin dir on PATH (setup SUCCEEDED and the step still skipped — **provisioned is not
reachable**, and the whitelist there keys on tool basename); and a missing espflash is now FATAL at both sites,
since a warning would turn a broken host into a green lane with no artifacts (#181's shape). The 0439
lane-narrowing skip is untouched — it keys on `NROS_FIXTURE_COORDS`, not on the tool. Verified both directions:
three 4.2 MB images produced, and exit 1 with a remedy when the dir is stripped from PATH. No `system = [...]`:
`serialport v4.9.0` builds with no libudev present, probed rather than assumed. See `archived/0486-*`. (2026-08-10)


Recently resolved (2026-08-10, phase-340 P2): **#490** — `packages/rmw/cffi/build.rs` declared
`cargo:rerun-if-changed=../nros-rmw-abi/include/nros`, a path that **does not exist** (the headers are
`packages/core/nros-rmw-abi/…`; phase-321 W2.e `12c365774` moved the crate out of `core/` and the relative
path came along). Cargo treats a missing rerun-if-changed input as PERMANENTLY dirty, and `nros-rmw-cffi`
sits under every image — so **every Rust fixture in the repo recompiled its whole chain on every build**,
silently, and every `check-fixtures-stale` run reported those rows as "STALE and have now been rebuilt".
Found by reading `CARGO_LOG=cargo::core::compiler::fingerprint=info` when a freshly built fixture probed
stale. Fixed + gated (`check-build-rs-rerun-paths`, self-testing, swept all 57 build scripts — one hit).
See `archived/0490-*`. (2026-08-10)

RESOLVED 2026-08-10 — **#491** (build, resolved 2026-08-10, phase-340) — two rows in the SAME shared cargo group could not both
be fresh. Cargo compares `rerun-if-env-changed` values TEXTUALLY, and one directory has three spellings
here: a leaf's `[env] … relative = true` (per leaf: `…/talker/../../../…` vs `…/listener/../../../…`),
`just/sdk-env.just`'s absolute export, and unset — so each sibling AND each build-vs-probe pair re-ran the
board + zpico build scripts and everything above them. Per-leaf `target/` dirs hid it; sharing the dir
surfaced it. Fixed by watching the CONTENT (`rerun-if-changed` on the canonicalised dir) and never the env
string, in BOTH producers — the Rust literals and the `rerun_if_env_changed` lists in
`config/*/nros-platform.toml` (fixing only the first left every ThreadX row still rebuilding 6 units).
Gated: `check-path-env-fingerprints`. Measured 6 → 0 units per no-op probe on freertos and
threadx-riscv64; 3.95 s → 0.12 s per probe in a controlled A/B. Also removed a false-FRESH: the one
FreeRTOS row with no `[env]` block made the board build script PANIC outside `just`, which
`rust-fixture-stale.sh` (stderr to `/dev/null`) read as "not stale". See `archived/0491-*`. (2026-08-10)

RESOLVED 2026-08-13 — **#488** the second-build-path RESIDUE: sites writing a per-leaf cargo dir that
`check-example-leaf-target-dirs` does not cover. Residues 1-3 (wake-latency, rtic-run-plan-e2e,
qemu-smoltcp-bridge, ros-editions, the three run-recipes, stack-analysis) moved to manifest rows, shared
groups or derived roots. Residue 4 — NuttX apps compiling INTO the source tree — is fixed by
`PREFIX := $(APPDIR)/external/.nros-build/$(CONFIG_ARCH)/`: `$(APPDIR)` because make resolves `$(CURDIR)`
through the staging SYMLINK to the physical dir, which is how a user's `make` wrote into the nano-ros
install; the arch because one apps tree serves both. Verified on the documented user flow
(`CONFIG_NROS=y` + `make -j8` exits 0, objects under `.nros-build/arm/`, source tree clean), and the
interim `.gitignore` ledger is deleted. Doing it uncovered three unrelated breakages on that flow — a
stale `packages/core` cargo path, a `+=` reading `NANO_ROS_ROOT` before it was defined, and a missing
mirrored object dir — all rooted in NO defconfig setting `CONFIG_NROS`, now covered by
`check-nuttx-integration-makefile` + `just nuttx build-integration-app`. See `archived/0488-*`. (2026-08-13):** the wake-latency pair,
`rtic-run-plan-e2e` and `qemu-smoltcp-bridge` are `[[fixture]]` rows resolved through those rows; the
freertos / threadx-linux / threadx-riscv64 run-paths and the cyclonedds make-driver leaf use the shared
group; `ros-editions` and `stack-analysis.sh` take a derived `<root>/<kind>/<coordinate>` instead, because
they regenerate `generated/` per edition / build with `-Z emit-stack-sizes` on nightly and must NOT share a
lane group. Moving the paths found two bugs they were hiding: `rtic-run-plan-e2e` had a row all along, so
the test `-kernel`ed a second leaf copy the freshness gate never saw; and `ros-edition-pose-pub` is rebuilt
per EDITION into one leaf dir with a consumer that never named the edition, so every edition overwrote the
last. **Still open for residue 4 alone:** NuttX's apps `make` compiles example sources IN PLACE with the
absolute build path baked into the object name. CORRECTED 2026-08-13 — the earlier "no `--target-dir`-shaped
fix applies" reading was wrong: `Application.mk` documents `PREFIX` as its out-of-tree hook and prefixes
objects, `.built`, `.depend` and `Make.dep` with it, and the Makefiles that set it
(`integrations/nuttx/Makefile`, `apps-external-template/Makefile`) are ours. The mangled `$(CWD)` SUFFIX
is separate and load-bearing upstream (it keeps objects unique inside one `libapps.a`). See `0488-*`. (2026-08-13)

Recently resolved (2026-08-10, phase-340): **#485** — `check-artifact-identity-budget` counted one crate
as TWO. `uniq -c` collapses only ADJACENT duplicates and glibc `en_US.UTF-8` collation ignores the space and
underscore, so `nros 079…` and `nros ecf7…` sorted on either side of `nros_board_common` / `nros_core` /
`nros_cpp` and the crate was reported as `7` and `5`. The tree-wide ceiling compared each half against 9 and
never saw the real **12** — a crate 33 % over the ceiling passed silently on every run since the gate landed.
It also blocked phase-340 item 8 on explaining a `worst crate` figure that moved 5→6→7 across sessions: there
was no drift, only the run boundary moving as hashes changed. And it was one hash away from crashing the gate
(`crate_identities` returns two lines for a split crate, and `[ "$n" -gt "$k" ]` on two lines is a bash syntax
error; `nros_core` stayed contiguous only because its hashes start 0/4/6/9). Fixed with a one-pass awk array —
not `LC_ALL=C`, which configures around a gate that reads plausibly while being wrong — plus a self-test on
every run, since nothing about a wrong reading looks wrong. First honest numbers: `nros_core` 8→**4**, worst
crate 9→**12** (= 2 workspace roots × 2 R3 halves × 3 feature identities, fully decomposed).
See `archived/0485-*`. (2026-08-10)

Recently resolved (2026-08-10, phase-342): **#484** — the ThreadX-rv64 RUST image took **2.11 s** to print `Subscriber created for topic:`
against **0.10 s** for the C and C++ images, on the same test file, the same QEMU invocation and the same RMW.
Cause: a `tx_thread_sleep(200)` "network stabilisation delay" in the RUST entry wrapper only
(`nros-board-threadx/src/entry.rs`, two sites) — at `TX_TIMER_TICKS_PER_SECOND = 100` that is exactly 2.00 s,
the whole gap. C/C++ never paid it because their `main` calls the nros-c API directly and skips that entry.
The wait was inherited "matching the legacy per-overlay wait", not measured, and the link it waited for is
already UP at 0.07 s — before the app thread starts. Deleted both sites: the cell went 5.31 s to 1.407 s and
the three languages now land within 100 ms of each other, which is what a shared platform crate + shared board
crate + a C API that thinly wraps the Rust one should have produced all along. **The asymmetry was the bug;
the four seconds were only how it announced itself.** Invisible until phase-342 W8b replaced the test's fixed
`sleep(4s)` with a wait on the readiness marker — 4 s was longer than either image needed, so 0.1 s and 2.1 s
looked identical. See `archived/0484-*`. (2026-08-09)

Recently resolved (2026-08-10, phase-342): **#480** + **#481** — readiness greps used string LITERALS, so a
wrong marker burned the whole timeout in silence and the test still passed. 0481 found it by MEASUREMENT
(`rust_cyclone` at 34.1 s against `cpp_cyclone`'s 5.2 s, because a settle greped `"Waiting for"` — which C/C++
print and the Rust listener never does), fixed 11 sites and landed the
`check-readiness-marker-literals` gate with **32 baselined**. 0480 carried the audit. Closed together by
converting all 32: each mapped to the binary its site spawns, then pointed at that binary's `output::*`
constant, so the baseline is **empty** and the gate enforces the rule with nothing exempted. **Three were
real defects, not ambiguity** — `zephyr.rs:860`, `zephyr.rs:1443` and `interop_e2e.rs:383` `.expect(…)` on a
marker their binary does not print, i.e. failing outright since 0471 made the wait strict. The other 29 were
correct-by-luck, which is what the class IS. Also collapsed TWO SPELLINGS rather than adding a constant per
spelling: `serial-listener` + `custom-transport-listener` said `Subscriber created on` where everything else
says `Subscriber created for topic:` — a `SERIAL_LISTENER_READY_MARKER` was written, immediately made
`"Subscriber created"` ambiguous, and the gate flagged a 30th site that had been invisible; the constant was
deleted and both binaries converged instead (phase-342's listener convergence, one binary further).
`WS_C_LISTENER_READY_MARKER` → `LISTENER_WAITING_BANNER` (four non-workspace binaries print it, so the
workspace-only name invited exactly that second-spelling problem). Gate verified both ways.
#480's earlier claim that this explained the 29 ci-matrix reds stays RETRACTED — those are fixture coverage
and staleness. What it explains is the silence. See `archived/0480-*`, `archived/0481-*`. (2026-08-10)

Recently resolved (2026-08-10): **#479** (examples) — `5f4eda8a4` fixed issue 0453 (an action server whose
output ignored the goal payload) on the **native** cells only, leaving the defect live on 8 of 10 cells.
Propagated in `46a8fe1d3`; `example_portability` is 6/6, so no `KNOWN_DIVERGENCE` was needed. **The risk
that kept it filed did not survive contact:** the embedded C copies already had
`server_context_t* ctx = (server_context_t*)context;` in `execute`, so only the `accepted_order` field and
its store in `goal_callback` were missing, and the four copies per language were byte-identical to each
other — one edit replicated, not eight judgment calls. C++ needed only the loop bound (`i <= goal.order`),
so an order-N goal now yields N+1 elements like its Rust and C siblings. The issue's own open question is
answered YES: 0450's Rust `State` did reach all four embedded Rust copies, checked per copy.
See `archived/0479-*`. (2026-08-10)

Recently resolved (2026-08-08): **#478** (build) — cc-rs sent `-mno-omit-leaf-frame-pointer`, a **clang** flag, to
`arm-none-eabi-gcc`, which rejects it — every `freertos` fixture row died `rc=101` while the other six
modules passed, and it was the last gate on `just ci-matrix`. Nothing in-tree passed it: cc-rs adds it when
forcing a frame pointer off `debug = 1`, and it arrived with no commit behind it because the lock pinning
`cc` here is the mixed workspace's, generated per host and untracked. Fix: `gcc_safe_frame_pointer` drops
cc-rs's automatic pair and re-adds the half gcc understands (clang/MSVC untouched), called from INSIDE
`strict_decls` — the one function every nano-ros C compile already calls — so ~20 sites were fixed with no
call-site edits; 7 unrouted sites in the two freertos board crates name it directly. Gated by
`check-cc-build-policy`, tripwired both ways. Verified: `lane=tier2` all eight modules OK.
See `archived/0478-*`. (2026-08-08)

Recently resolved (2026-08-08): **#477** (nuttx) — `nuttx-c-talker-zenoh` overflowed ROM by 448776 bytes and gated
`lane=all`/`lane=tier2`. **Not a size regression at all.** `nros-board-common`'s `snapshot_root` /
`snapshot_or_tree` prefer the per-arch NuttX export snapshot and fall back to the live `staging/` tree, but
emitted `cargo:rerun-if-changed` on the path that WON — so a build that ran before the ARM export existed
pinned its edge to `staging/`, and the snapshot later appearing changed nothing it watched. A `lane=all`
sweep builds RISC-V and ARM, so the board artifact stayed staged against the wrong tree. Fix: emit the edge
on the preferred path even on the losing branch (a `rerun-if-changed` on a missing path still fires when it
APPEARS). Proof: after provisioning + `cargo clean` of the two crates, HEAD linked with the same code —
691468 bytes vs 687112 on Aug 6, +0.6 %; the ARM lane now completes with zero overflows. The bisect never
took a step: the confounder surfaced while validating the endpoints, which is why both ends get validated
first. See `archived/0477-*`. (2026-08-08)

Recently resolved (2026-08-08): **#483** — all 16 `emulator.rs` tests skipped in ~0.06 s on a missing
`build/qemu-zenoh-pico/libzenohpico.a`, and the skip named `just build-zenoh-pico-arm`, a recipe that never
existed. `just qemu build-fixtures` succeeded without building it, so the lane reported a green meaning "did not
run": 0 passed/16 skipped in ~1 s versus 16 passed/0 skipped in 116 s once built by hand. FIXED at both ends —
the message now names `just qemu build-zenoh-pico` (which `just qemu setup` already called, so only the message
was wrong), and `build-fixtures` now builds it idempotently, because "I built the fixtures" is exactly when the
suite is expected to run. See `archived/0483-*`.

Recently resolved (2026-08-08, phase-340 W3): **#482** — after a clean, fully successful
`just build-test-fixtures lane=tier2`, `just ci-matrix` produced ~231 STALE/not-found failures; the same
tree rebuilt `lane=all` dropped to ~19. Two "computed twice" defects, the issue-0196 / #393 family applied
to lane membership. (A) A row's `(platform, lang, rmw)` coordinate was computed in two places with two
answers — `matrix_fixture_coverage.rs` read an omitted `rmw` as `zenoh`, `fixtures-manifest.py` read it as
`None` and matched nothing — so **67 of 240 buildable rows sat in NO coordinate-scoped lane** (tier 2
selected 46/240 where it should select 109/240; the 63 missing rows were every native Rust example and
every bench, including the one in the symptom). The staleness gate used the SAME filter, so it could not
report what the build had skipped. (B) `ci-matrix` never narrowed its RUN, so BUILD ⊉ RUN, and
`_require-fixtures` accepted a `lane=tier2` stamp for a run that executed everything. Fixed in two steps:
2026-08-07 made it HONEST — one `row_coord()` consumed by both sides, and `CiLane::run_scope()` deriving
the required build (`all`) from the run, so `ci-matrix` fails at preflight in seconds. 2026-08-08
(phase-340 W3) made it CHEAP — the RUN now narrows to the same coordinates in the fixture RESOLVER
(`NROS_TEST_COORDS` → `nros_tests::fixtures::lane`), which attributes an artifact back to its manifest row
through `row_artifact_root()`, the sibling of `row_coord()`. Build-set and run-set are then one predicate
on one coordinate file, `nros_lane_build_lane` maps tier 2 to itself, and
`just build-test-fixtures lane=tier2 && just ci-matrix` is the supported pair. The skip is keyed on the
COORDINATE, never on "artifact absent", so an in-lane fixture that is missing or stale still fails hard
(issue 0445). See `archived/0482-*`. (2026-08-08)

Recently resolved (2026-08-07): **#475** — the RMW archive was an ORDER-ONLY (`||`) link dep of the C/C++
example binaries, because it is whole-archived through a raw `-Wl,...` FLAG (CMake cannot see a file inside a
flag string) and `add_dependencies()` supplies only build ORDER. A backend edit rebuilt the archive and never
relinked the examples — museum binaries by construction, clearable only by `rm -rf` (~687s per Cyclone leaf).
The staleness probe was RIGHT; the graph was wrong, and the remedy the probe printed could not work. FIXED
with `LINK_DEPENDS` in `nano_ros_link_rmw` — the file edge, without touching the link line. Verified by
touching a backend source and watching `c_talker` relink. Two non-fixes recorded in the issue:
`INTERFACE_LINK_DEPENDS` does nothing on CMake 3.22, and `target_link_libraries`-ing the archive BREAKS the
link (`undefined reference to ddsrt_*` — it reorders ld's single pass out of the whole-archive group).
See `archived/0475-*`.

Recently resolved (2026-08-07): **#474** — `just format` was not behind `_require-leaf-includes`, so on a
checkout with an unsynced leaf it died with cargo's raw manifest-parse error four frames deep, never
naming `nros sync` — blocking the very workflow CLAUDE.md says to run before broad changes. Not a new
bug: #0463 had established the cause and fixed the SEAM at `build-test-fixtures-leaves` and
`rust-rtos-link-check`, the two sites where it had been observed; `format` was a third site walking the
same leaves (issue-0196's rule that the gate must cover the new site). Fixed by `2a89b5040`, which also
cleared the clippy red. See `archived/0474-*`.

Closed as wontfix (2026-08-07): **#473** — filed claiming `nros sync` leaks `# nros-managed` patch rows
into tracked `.cargo/config.toml`, re-dirtying the worktree. **The premise was wrong**, and asking "why
are these tracked" gave the opposite answer. `.gitignore:92-105` states the design: a config that is PURE
sync output is untracked, but one carrying hand-authored content (`[build] target`, a QEMU `runner`, link
args) STAYS tracked and "sync only refreshes the patch block INSIDE them". Measured: 696 configs on disk,
75 tracked, **50 of those carry managed rows** (133 rows) — the documented category, not a two-file leak —
and every row is a repo-relative path to a COMMITTED in-tree crate, with **zero** naming a `generated/`
tree, so they reproduce identically from a clone. `check-cargo-config-tracked` passes because it is
correct, not because it is too narrow. #457's host-specific split into `nros-managed-patch.toml` is
holding. The dirt observed was a transient delta (a new dep, `nros-zephyr-build`, added a row not yet
committed); at rest the worktree is clean. SURVIVES independently: those two zephyr entry leaves name
`nros-zephyr-build = "*"`, a registry name for an in-tree crate — #378's public-crates.io exposure —
which path-dep'ing would remove along with the patch row. See `archived/0473-*`.

Recently resolved (2026-08-07): **#474** — `just format` died on an unsynced leaf with cargo's raw
manifest-parse error, never naming `nros sync`, blocking the very workflow CLAUDE.md says to run before
broad changes. Not a new bug: #463 fixed the SEAM at the two sites where it had been seen
(`build-test-fixtures-leaves`, `rust-rtos-link-check`) and `format` was a third walking the same leaves —
the issue-0196 rule about a gate's coverage being narrower than its rule. FIXED by adding
`_require-leaf-includes` to `format`. Siblings audited rather than fixing the reported site alone:
`check-example-fmt` is NOT exposed (it calls `rustfmt` per file via the git index and parses no manifest),
`native::check` IS exposed by the same mechanism but is unreached by `just check` and left noted rather
than guarded. Caveat for re-testers: the leaf that exposed this was re-synced by an unrelated NuttX build,
so the original symptom no longer reproduces here. See `archived/0474-*`.

Recently resolved (2026-08-07): **#471** — `wait_for_output_pattern` returns `Ok` on TIMEOUT whenever the process printed anything at all;
the pattern is consulted only for the early-exit path. So `wait_for_output_pattern(MARKER, …)?` means "the
process was not silent", NOT "the marker appeared". **233 of 283 call sites** ignore the returned string and
check only the `Result`. RESOLVED (2026-08-07): the contract is now strict — `Ok` means the pattern appeared,
`Err` quotes the output — with `collect_until()` as the lenient counterpart under an honest name; both share
one `(String, bool)` engine, since conflating "what was printed" with "did it match" WAS the defect. The same
two lenient paths existed in `QemuProcess` and were fixed with it. The flip caught exactly one class, 15-16
tests: suites waiting for the literal `"Waiting for"`, a banner `examples/native/rust/listener` stopped
printing at phase-277 — now `output::LISTENER_READY_MARKER`. Those suites also got 2-3x faster, having been
burning a full 5 s timeout per listener. See `archived/0471-*`. (2026-08-07)

Recently resolved (2026-08-07): **#476** — writing an executable stub and exec'ing it races against sibling test
threads — `O_CLOEXEC` closes a descriptor at EXEC, not at FORK, so any concurrent `Command::spawn` inherits
the still-open write handle and our `execve` gets `ETXTBSY`. **Unique paths do not fix it** (that was #455's
cause, and it was already fixed here — the failing path was pid-scoped). Measured 245/1200 execs failing at
12 forking threads. Fix: `test_support::write_executable_stub` writes via CHILD `cp`+`chmod`, so no write
descriptor ever exists in our process — 0/1200. See `archived/0476-*`. (2026-08-07)

Recently resolved (2026-08-07): **#469** — phase 209's three C++ port templates were in NO lane (0 fixture
rows, 0 tests, 0 recipes), so nothing built or ran them for two months while the acceptance silently broke
(#0465). RESOLVED: three `cmake-configure` fixture rows + `port_templates_e2e` asserting the vendored ROS 2
tutorial node actually publishes. A build row alone would NOT have caught it — the template compiled and
linked cleanly the whole time it was broken. The test was verified to FAIL against the pre-fix shim before
being trusted, which is how the `wait_for_output_pattern` trap surfaced. See `archived/0469-*`.

Recently resolved (2026-08-07): **#468** — `TransportError::InvalidConfig` had no ABI code: it encoded to
`NROS_RMW_RET_INVALID_ARGUMENT` and decoded back as `InvalidArgument`, so a capacity the BUILD cannot honour
(an exhausted `ZPICO_MAX_SESSIONS` pool) arrived looking like a caller passing something wrong — opposite
remedies: fix the call vs rebuild. Last hop of #0465's collision. RESOLVED by adding
`NROS_RMW_RET_INVALID_CONFIG = -19` following the `-18 CONNECTION_FAILED` precedent, plus both mapping
directions and regenerated bindings. Verified: `Err(Transport(InvalidConfig))` end to end.
See `archived/0468-*`.

Recently resolved (2026-08-07): **#465** — phase 209's acceptance template built and linked but did not RUN
(`Transport(ConnectionFailed)`). Root cause was NOT transport: the rclcpp shim opened TWO sessions —
`rclcpp::init()` builds the global executor, and `rclcpp::Node` then called `Executor::create()` for its OWN.
A non-bridge app has exactly one session; two is the bridge shape. With `ZPICO_MAX_SESSIONS` at its default 1
the second open exhausted the pool and returned -1, wearing the same error text a real connection failure gives.
FIXED in the shim, not the pool: `Node` now creates on the global executor (`::nros::create_node`), spins via
the free `::nros::spin_once`, and no longer shuts the shared session down per-Node; `executor_` and
`nros_executor()` are gone. Verified at the SHIPPED default — ONE open, and the README's expected output.
Raising the pool would have hidden a design error behind memory spent on every embedded target. STILL OPEN:
the templates are in no lane (0/0/0), which is why this sat unnoticed since 2026-05-30 — and a build-only row
would not have caught it. See `archived/0465-*`.

Recently resolved (2026-08-07): **#465** — phase 209's acceptance artifact
`examples/templates/cpp-port-minimal-publisher` compiled and linked but no longer RAN
(`Transport(ConnectionFailed)`) while a shipped `cpp_talker` reached the same router. Root cause was NOT
the usual missing backend: the rclcpp shim opened a SECOND RMW session and the pool ships with one, and
an exhausted pool was being reported as a connection failure. It had rotted unnoticed because none of
phase 209's three port templates sat in any fixture row, test or recipe — `just check` compiled them,
which kept the build half true while the run half died silently (issue 0317's shape, 0196's rule with no
gate). Fixed by `4b30c29cb` + `8151819b7`, and `390f4f9eb` gave the port templates a lane that can fail.
See `archived/0465-*`.
Recently resolved (2026-08-06): **#444** — every FreeRTOS **Rust** cell (pubsub, service AND action —
the issue's action-only scope came from one run and was wrong) booted, printed "Network ready." and
stalled forever. Reproduced on main with fresh fixtures, so not branch-specific. TWO faults, both from
the phase-338 W2 `-entry` collapse (`ab486a8db`). (1) The deploy block kept `tcp/10.0.2.2:7447` and no
`ip`/`gateway`, so lwIP came up on the board's STATIC 192.0.3.10 while the harness launched default
slirp — unroutable, and 7447 is a port the harness never serves; gdb put it in a blocking
`_z_open_link`. (2) One ZID for two peers: the platform PRNG is (ip, mac)-seeded because zenoh-pico's
ZID comes off it, both images booted the same pair, and the router keeps ONE peer (`max_links=1`) —
what `NROS_ENTRY_IP_LAST` 10/11 has always prevented for C/C++. RESOLVED upstream by `07faa2383`
(ip .15/.16 + gateway + per-variant port, restoring the default-slirp plan the launcher expects); an
independent fix from the other direction (move Rust onto the C/C++ board-net plan, unify the launcher)
was dropped in its favour. All NINE FreeRTOS cells pass. Still open underneath: `rtos_e2e` keeps the
Rust lane on a SEPARATE network plan from C/C++ on the same board, and its own comment calls unifying
them follow-up work — a lane whose firmware config and launcher are maintained apart is a lane where
they can silently stop matching. See `archived/0444-*`.

Recently resolved (2026-08-07): **#463** — two design comments in `cmd/ws.rs` asserted that "cargo
ignores a missing `include` SILENTLY". On cargo 1.97.1 it is a HARD error raised during *manifest
parse*, so a leaf whose include target is absent cannot be READ — `cargo metadata` and every gate that
walks it fail too, four frames deep, naming a path that never mentions sync. Both generated targets are
gitignored, so a clone has neither: the central `nros-patch.toml` (57 leaves, since #272) and the
`nros-managed-patch.toml` sidecar (48 leaves, since #457) — i.e. the hole predates #457, which added a
second instance rather than causing it. NOT fixed by making a clone parse: committing the targets would
put ament-derived rows in git (the churn #457 removed), cargo has no optional include, and these leaves
cannot build before sync anyway since their patches name `generated/` trees only sync writes. RESOLVED
at the seam instead — `_require-leaf-includes` says "run `nros sync`" before cargo says anything, and
`check-cargo-config-tracked` now rejects an include naming a target NO generator writes (which no sync
run could ever satisfy — verified to fire, not merely to pass). See `archived/0463-*`.

Recently resolved (2026-08-06): **#457** — sync's managed `[patch.crates-io]` block sat inside the leaf
`.cargo/config.toml`, which is tracked: it committed host-derived `generated/` paths AND re-dirtied the
worktree on every sync as the row set moved. RESOLVED by splitting the file on its real seam — the block
now lives in the gitignored sidecar `.cargo/nros-managed-patch.toml` reached by a second `include`, while
the authored half (`[build] target`, QEMU `runner`, link rustflags) stays tracked because a clone cannot
regenerate it. NOT the filed proposal (render the whole config from `nros-board.toml` + gitignore it):
measured per line, only **1 of 48** board-resolvable leaves equals its board's `cargo_config` — leaves
legitimately override the `runner` and add `[env]`, which no descriptor can express, so that plan would
have deleted content with no other home. Also corrected: deploy tokens are NOT board names (the working
mapping is the board-crate dep, 54 leaves, 0 ambiguous), and three tracked configs outside `examples/`
DID patch generated msg crates — `tests/` was outside the gate's walk. `check-cargo-config-tracked` now
walks it and rejects a tracked config patching an uncommitted `generated/` tree. See `archived/0457-*`.

RESOLVED 2026-08-10 — **#460** three `zephyr/rust` entry cells died after "Network ready" with a bare
`NodeRegister("lifecycle")`. Root cause: **a service server IS a zenoh queryable, and the table holds 8.**
The macro registers `[param_services]` (6) before `[lifecycle]` (5), so the ninth declaration is refused —
eleven capability services against an 8-slot table. Three things hid it: the overflow's only diagnostic was
`cfg(feature = "std")` (off on every embedded image, the only place the 8-slot budget applies); seven
`map_err(|_| ServiceServerCreationFailed)` sites flattened the reason away; and
`CONFIG_NROS_MAX_QUERYABLES` never reached the RUST lane — the `MAX_CBS` finding generalized, since cmake's
`set(ENV{…})` knob exports reach the C lane's re-baked command and zephyr-lang-rust's
`rust_cargo_application` builds its own and inherits nothing (measured: `.config` 16, cmake TU 16, crate
const 8 — also an 0135 ABI split). Fixed with ONE `nros_zephyr_build::knob_usize` ladder used by all four
knob readers, gated by `check-kconfig-knob-forwarding` (21 knobs); the overflow now returns a `&'static str`
`Backend` reason that crosses `no_std`. The third cell was a stale ASSERTION, not a fault: a `params_files`
overlay phase-331 W3 put on that component makes the resolved value 120, not the launch inline's 250.
`entry_matrix: 14 ran, 1 skipped, 0 failed`. See `archived/0460-*`. (2026-08-10)

Recently resolved (2026-08-10): **#451** — a bare `cargo build` in an embedded example died on a missing
env var, one at a time, in a way that reads like a code fault (on NuttX it reaches the LINKER as
`undefined reference to open / socket / malloc`). RESOLVED by phase-345 W1, with the issue's own CAUSE
corrected: `activate.sh` DID have a delivery mechanism (it sources `scripts/sdk-env.sh`, which evaluates
the `just/sdk-env.just` SSoT) — what it lacked was coverage, because three separate lists decided which
variables survived and each disagreed with the SSoT. Measured in a clean environment: bash 14/23,
fish 2/23 (an `NROS_*` import filter dropped every third-party SDK root), zsh 0/23 (bash-only `${!name}`
→ `bad substitution`, plus a bash-only sourced-vs-executed test). All three now 23/23, each deriving the
list from the SSoT. `check-activate-shells.sh` passed throughout — it asserted COMPLETION, not delivery —
and now asserts both, with probes under `env -i` because a direnv host was feeding the probe 22 of the 23
it was checking. See `archived/0451-*`. (2026-08-10)

Recently resolved (2026-08-10): **#452** — embedded builds regenerated `nros_generated.h` /
`nros_cpp_ffi.h` with a DIFFERENT cbindgen, so any embedded lane silently dirtied tracked headers, and
committing it REVERTED the C23 enum-base guard (hand-reverted twice during phase-338). RESOLVED by
phase-345 W3, with three corrections to the issue's own text: there are **three** such headers (the sweep
found `zpico-sys/c/include/zpico.h`), the pin is an **exact cargo requirement** rather than a
`.clang-format-version`-style file (cbindgen is a dependency, so cargo's resolver IS the pin, and it binds
the lockless leaves a lock cannot reach), and the drift was **already committed** — two tracked NuttX FFI
leaf locks pinned `cbindgen 0.29.4` against the root's 0.29.3. Builds now COMPARE and warn;
`just regen-c-headers` is the only writer; `check-cbindgen-pin` + `check-cbindgen-headers` gate both halves.
Acceptance discharged on the issue's own repro: `just nuttx build-examples` green, all three headers
untouched, zero stale warnings. See `archived/0452-*`. (2026-08-10)

Recently resolved (2026-08-06): **#445** — a staleness verdict is TERMINAL and self-explaining, so
it absorbs whatever the fixture would have done at runtime. Demonstrated: issue 0442 made the
freertos/threadx C/C++ cells read stale; fixing it made them run and immediately exposed issue 0444, a
real FreeRTOS Rust runtime failure that had been sitting behind it. I hit the reader-side half too —
asked why those cells were stale I gave a plausible, consistent, WRONG answer, because the verdict
explains itself well enough to survive scrutiny. RESOLVED without weakening any probe: the exemption
rule now has ONE spelling (`fixtures::staleness::exempt_probe_input`, which 0442's own fix had left
divergent on a THIRD arm), every verdict prints what it examined and exempted, and a per-coordinate
ledger counts consecutive non-running resolutions — from the second, the message says the runtime
result is being absorbed. `just fixture-staleness` lists them; `check-staleness-probe-exemptions`
rejects a second spelling or a probe that does not account, report and clear. See `archived/0445-*`.

Recently resolved (2026-08-06): **#442** — the regenerated-header exemption was applied on the ninja
dep-info arm of the cmake freshness probe but NOT on its sibling directory-walk arm, so every
freertos / threadx-linux C and C++ zenoh fixture read STALE against `zpico-sys/c/include/zpico.h` —
a cbindgen header written in place, whose mtime moves on any other feature set's build with the
content unchanged (measured: header 23:46 vs binary 21:23, `git status` clean). Issue 0196 one layer
in. RESOLVED: `newest_source_after` skips it like the loop does; action cells went 3 → 7 of 9 passing.
First written off as "a core-crate change staled main-built fixtures" — plausible and wrong, since the
observable is identical; reading the actual `newer:` path settled it. See `archived/0442-*`.

Recently resolved (2026-08-06): **#440** — phase-338 W2's `-entry` collapse kept the NODE package's
`.cargo/config.toml` and dropped the board's STATIC link args, so all six NuttX Rust entries failed
with ~3680 undefined libc references (`grep -c lsched` was 0 of 6). RESOLVED: the 24 args restored
from `nros-board.toml`'s `cargo_config` (the SSoT, not the deleted files), rustflags only —
`build-fixtures-arm` RC=0 with 0 undefined refs and all three nuttx action cells (Rust/C/C++) pass.
Gated by `check-board-cargo-config-applied`, watched to fire. Nothing caught it because the broken
config was valid TOML that cargo and `nros sync` both accepted — the loss showed only at link time,
on one platform. See `archived/0440-*`. (2026-08-06)

RESOLVED 2026-08-07 — **#459** the Zephyr C++ realtime entry reportedly emitted nothing after boot
(4 lines), reported as a missing EDF marker. Does NOT reproduce: on a rebuilt image it emits 1644 lines
including both `[nros] tier task entered` and the EDF line, and `sched_dims_applied_e2e` passes 12/12.
The fixing change is NOT identified and is not claimed — `73a3c4e44` (#458, unstamped C++ handle tag so
every tier setup got -3) matches the mechanism exactly, but the working image predates it by most of a
day, so the likeliest reading is that the reported run used a pre-rebuild image. Also corrected: the
rust realtime image is not missing, it is `build-ws-rs-` not `build-ws-rust-`. Closed as
not-reproducing rather than fixed, because the difference matters if it returns. See `archived/0459-*`.

RESOLVED 2026-08-07 — **#461** an action server read a wrong `order` for every goal, differently per
language: Rust `1`, C/C++ `256`. The filed guess ("something structural, e.g. a CDR word") was half
right — `1` is the GOAL COUNTER (`goal_id_from_counter` writes it LE into the uuid's first bytes, so
goal two would have read `2`; the "constant" came from only ever looking at goal one). Dumping the wire
found TWO bugs. (1) `TickCtx::send_goal` serialized the goal `new_with_header` and handed it to
`send_goal_raw`, which frames `[header][uuid][those bytes]` itself — a SECOND encapsulation, 28 bytes
where ROS 2's SendGoal_Request is one message with one (24). The `action-client-multigoal` fixture used
the typed handle and sent the correct 24, which is why IT decoded fine and the example never did.
(2) `CallbackCtx::message()` deserialized from the uuid, independent of (1). C/C++ needed no change.
Recorded in the issue: my first attempt fixed it server-side at four seams, made all three languages
read 10, and took `action_multigoal` from 4 accepted goals to 0 — the wire dump is what reduced it to
two one-line changes. Guarded by `goal_order_reaches_the_server`, which asserts the value ROUND-TRIPS;
every pre-existing action test asserted delivery markers only. 5/5 pass. See `archived/0461-*`.

RESOLVED 2026-08-06 — **#450** the group-A action-server published a fixed `[0, 1, 1]` whatever the
request, and converging riscv64 had deleted the only body that computed anything. RESOLVED: the group
body now computes the sequence iteratively and streams one feedback frame per element, across all six
platform copies at once (the portability gate makes them move together), with buffers 128 -> 256 and a
`MAX_ORDER = 50` bounding both the `heapless::Vec<i32, 64>` and the 256-byte CDR payload. The requested
order is now honored — which the deleted riscv64 body did NOT manage either (it used a fixed
`ORDER = 5`, because `for_each_active_goal_for_name` yields no request payload); the accepted order is
carried from `on_goal` to `tick` through `State`. Verified: portability 6/6, native actions 4/4,
freertos + nuttx `build-examples` clean, freertos action Rust/C/C++ 3/3 on the Cortex-M3. This is what
uncovered **#0461**. See `archived/0450-*`.

RESOLVED 2026-08-06 — **#441** `test_zero_copy_message_info` had nothing to observe: the zero-copy and
plain listeners produced identical output. Neither obvious repair worked — `CallbackCtx` exposes NO
MessageInfo accessor, so the `seq=` line the test grepped had never come from the receive path (that
shape lives on the executor's `.message_info()` builder, which the Node API a demo is written against
never reaches), and adding a `cfg` branch to the example would break phase-338 W1's byte-identical
portability gate across seven platform copies. RESOLVED by moving the assertion rather than weakening
it (retargeting at the publisher's trace, as #0429 did, would have gone green while no longer testing
the receive path): a `message-info-observer` bin registers through `.message_info()` and emits both the
standard `I heard:` line and `seq=/gid=/ts=`, with a loud ABSENT error if MessageInfo is missing — a
quiet skip would read as "no messages", this issue's own failure mode. The marker is a CONSTANT, and
the fixture pair differs in exactly `unstable-zenoh-api`, which is what makes it a trampoline check.
All THREE zero_copy tests were broken the same way (the other two waited on `"Waiting for"`, also
slimmed away); 3/3 pass, monotonic seq and stable GID measured through the zero-copy path. See
`archived/0441-*`.

RESOLVED 2026-08-06 — **#438** `native_orchestration_tiers` failed on "no multi-tier marker", and the
filed diagnosis (linux board missing the NuttX marker) was wrong twice over. The linux board HAS both
markers — the issue's grep missed them because the string is line-broken across a `\` continuation, so
an issue about grep drift was itself produced by a grep artifact. And the binary never reached that
code: `strings` found zero `multi-tier`, `nm` no `run_tiers` symbol. REAL CAUSE: `resolve_tiers` builds
tier membership only from group bindings, and phase-273 W2 moved binding from the package manifest to
`[[component]].group_tiers` — this fixture was never migrated, so its two authored tiers had no members,
collapsed to one synthesized `default`, and the macro emitted the SINGLE-tier path in silence while the
fixture's own doc-comment claimed to prove the multi-tier emit. RESOLVED: both tier fixtures bind their
groups (the freertos sibling carried the identical latent defect), the native one moves to the canonical
`launch =` arm (the deprecated `model =` arm has an empty membership map BY CONSTRUCTION and can never
resolve a multi-tier system), and the silent discard is now a compile error naming the tiers and the
remedy. 6/6 pass, including the with-router case the issue expected to need separate zenohd work — it
did not. See `archived/0438-*`.

RESOLVED 2026-08-06 — **#456** two of the three NuttX riscv recipes never exported the riscv env, so
the C lane archived an **arm** vector table into a riscv image. `build-riscv-rust` held the only copy
of the six arch-describing `NUTTX_*` values (including phase-285 W4's `NUTTX_VECTORTAB=""` opt-out);
`build-riscv-c` and `build-riscv-c-workspaces` took the helpers' qemu-arm DEFAULTS, and the live-tree
fallback happily handed them `arch/arm/src/arm_vectortab.o` left behind by the previous ARM build.
`ar` does not check machine types, so the link failed `cannot find -lnros_nuttx_boot` — `ld` skips an
incompatible archive and then looks no further, naming a missing file three steps from the cause. The
phase-285 W5 accommodation above the site is the same class absorbed rather than named: it tolerates
the wrong arch exactly as long as the file is missing, which stopped being true once both arches build
in one lane. RESOLVED: one `scripts/nuttx/riscv-env.sh` sourced by all three recipes, plus an ELF
`e_machine` check in `run_image_link` that fails naming both arches. Found while verifying #439/#443
end to end; it was the last thing between them and a green `lane=tier2`. See `archived/0456-*`.

RESOLVED 2026-08-06 — **#439** a lane-narrowed build killed any recipe naming a fixture by `--id`, so
three of eight tier-2 modules died, no stamp was written and `just ci-matrix` could not run at all.
Two guards each right alone: #0393 removes rows for LANE reasons, #0406 treats a zero-row `--id` as a
typo — together, a lane dropping the row made 0406 blame the caller, printing requested and declared
coordinates that were IDENTICAL because the lane appeared in neither. Fixed in `9c6420144` (re-query
without `--coords-from`: present ⇒ out-of-lane, exit 0; absent ⇒ real typo), closed here after
verifying all three reported invocations exit 0 AND that a genuine typo is still fatal with and
without a lane — the failure to fear was a guard gone lenient generally rather than lenient about
lanes. See `archived/0439-*`.

RESOLVED 2026-08-06 — **#443** the staleness gate ignored the run's lane, because the lane is two env
vars and `ci-matrix` set only one: `NROS_FIXTURE_SCOPE` fell back to `all` and the gate audited the
whole tier-3 set while the run, build and stamp were tier 2. `_check-fixtures-stale` now DERIVES the
scope from `NROS_FIXTURE_LANE` (verified: `scope=coords (lane:tier2) … 13 coordinate(s)`), so
`ci-matrix`, `ci-matrix-nightly` and the next lane recipe need no second variable. Closed with the two
pieces derivation alone does not give: the gate now PRINTS the scope it chose and where it came from
(the defect survived because a green line looks identical whether it covered 3 coordinates or 47 —
issue 0445's lesson on the build side), and an explicit scope that CONTRADICTS the lane now exits 2,
which closes the dangerous direction the issue named (a scope narrower than the lane launders a green).
`just ci` and `_lane-gate` unchanged; `NROS_TEST_SCOPE` stays independent by design. See
`archived/0443-*`.

RESOLVED 2026-08-06 — **#437** `check-fast` was RED on main: `check-build-profile-literals` flagged
four sites in `just/px4.just`. The PX4 SITL lane is deliberately release-only (the FFI archive and the
symbol fixture land in the SAME image; a split link takes half of each), so all four carry
`# profile-literal-ok: symbol fixture`. The PLACEMENT is the lesson: the window is 3 lines and the
flagged token sits on a `\`-continuation line where no comment can precede it, so the marker must be
inline or the LAST comment line before the command — a rationale block between the two puts it out of
range, which is how this failed twice, once while being fixed. Still uncoupled underneath: line 168
builds the archive and line 181 hands PX4 its PATH from separate spellings, so a profile move makes
them disagree and PX4 links a stale archive. See `archived/0437-*`.

RESOLVED 2026-08-06 — **#436** the PX4 uORB→RMW bridge now works end to end (uORB samples translated to CDR px4_msgs and published on zenoh, 100+ forwarded). Five defects fixed: TWO copies of zenoh-pico in one image (each with its own statics — z_open failed silently; zenoh-pico now has ONE source, the umbrella built with a platform feature); uORB registered under the deprecated unnamed shim so it could never be named; `open_multi`'s extra sessions were anonymous; two incompatible executor handles behind one `void*` (fixed by `nros_cpp_init_multi`, which opens multi-RMW sessions into a real CppContext); and four collapsed error seams. See `archived/0436-*`.

Recently resolved (2026-08-05): **#434** — FreeRTOS C++ TUs resolved `<nros/nros_config_generated.h>`
to the in-tree `#error` stub, so `freertos cpp` fixtures could not build. NOT the ordering race the
first diagnosis claimed (lifting the Zephyr-guarded `OBJECT_DEPENDS` changed nothing, and two builds
with the headers present failed identically): the include LIST order was wrong. phase-337 W5.b added
the SOURCE `packages/api/nros-c/include` to `FREERTOS_STARTUP_INCLUDES` for `app_config.h`, assuming
"the per-app generated header shadows this one" — it is shadowed BY it, landing at position 9 ahead
of the generated dirs at 10/13. Removed; the dir is still reachable as nros-c's INTERFACE include
(position 14). All three freertos action cells now pass. See `archived/0434-*`.

Recently resolved (2026-08-05): **#433** — `just nuttx build-fixtures` exits 0 and its fixtures read
STALE. Root cause: arm and riscv share ONE configured kernel tree, and the riscv half re-stages it
after the arm entries link (one run shows two full `Building NuttX...` plus two "up-to-date" skips).
Decisive test: `build-fixtures-arm` alone leaves the entry FRESH and all three nuttx action cells
pass. Root-caused and worked around, NOT structurally fixed — a per-arch tree or an arch-keyed
freshness signature is larger and belongs with the NuttX board owner. See `archived/0433-*`.


**#200** — fixture-build timing campaign blocked on a big-disk CI runner (phase-226 validation
residue). See `0200-*`.

Recently resolved (2026-08-06): **#435** — filed as "CMake fixtures do not depend on generated RMW
headers"; SUPERSEDED by **#442**, whose diagnosis is the correct one. Not a missing dependency:
`zpico.h` is generated IN PLACE by its own producer's build script, so it cannot be its own input.
The real defect was that `cmake_dep_info_newer_source`'s two arms disagreed — the ninja `-t deps`
loop skipped `REGENERATED_INPLACE_HEADERS`, its sibling walk did not — so the walk reported exactly
what the loop was written to ignore. Fixed in `2e333c068`. See `archived/0435-*`, `archived/0442-*`.

RESOLVED 2026-08-12 (`44e7f2354`, phase-346 W2+W3) — **#432** the pinned `zephyr-lang-rust` (404fcef) could not build the `zephyr` crate for ANY board
whose devicetree has gpio nodes: its DT generator emits a five-argument `GpioPin::new` against a
six-argument signature (`pin` without `dt_flags`). `CONFIG_GPIO=n` makes it worse, not better — the
generator reads the devicetree, and the `gpio-keys` augment carries no `cfg:` key, so the calls are
still emitted while the `raw` bindings vanish (14 errors instead of 4). Invisible until phase-337
W2.b added the first non-native_sim Zephyr board, native_sim having no gpio nodes. Since essentially
every real board has gpio, Rust-on-Zephyr is native_sim-only until this is fixed upstream; C and C++
are unaffected (no `zephyr` crate), which is why W2.b's cells build the C entry. See
`archived/0432-*`. (2026-08-05, resolved 2026-08-12)

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

**#260** — the RFC-0052 Native sched dims are e2e-verified only on the FALLBACK arm: every realtime
fixture is uniprocessor, so the SMP core-pin ACCEPT path (`k_thread_cpu_pin` /
`pthread_setaffinity_np` / `vTaskCoreAffinitySet`, `#ifdef CONFIG_SMP`) is compile-verified only.
Needs ONE SMP fixture (a separate zephyr native_sim SMP variant, not the shared image) to flip a
two-mode core-pin e2e to accept. See `0260-*`. (phase-296 W5.11 2026-07-24)

**#271** — Orin SPE BTCM footprint regressed ~+195 KB between `d9af52be` and `21a3a4248`; a
minimal `Executor::open`+spin image no longer fits 256 KB. See `0271-*`.

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

Recently resolved (2026-08-10): **#0371** — RESOLVED 2026-08-10, archived. See `archived/0371-*`.

**#374** (filed as #373; renumbered) — `nros setup native --rmw zenoh` source-builds zenohd
(`[tool.zenohd]` has no `dist.linux-x86_64`; assets never seeded) and pulls a SECOND rust toolchain
(`1.85.0`, zenoh's own pin) for 792 MB of store — while installation.md:123 promised "ships prebuilt
toolchains per platform per RMW" for exactly this board. **Narrowed 2026-08-01:** the wait is now
announced up front by `nros setup` and the book no longer promises unconditional prebuilts; what
remains is out-of-repo — publish `1.7.2-nros2` assets on `nano-ros-sdk` so the dist rows return.
See `0374-*`. (2026-08-01)

**#403** — the WCET bench emits prose nothing parses, and a QEMU run with a dead cycle counter reports
zeros as if measured. `nros-bench/wcet-cycles-qemu` prints `min=/max=/avg= cycles` to a semihosting log
with no parser, no schema and no consumer, in the `debug` group so no lane runs it. On QEMU the DWT
never increments: the bench detects this, prints a NOTE, then measures anyway and exits 0 — warning and
data on the same stream, and only the data is machine-shaped. Same failure as 0259 one layer earlier
(a non-measurement entering as zero, the most optimistic WCET there is). Direction: structured artifact
with conditions recorded, and a dead counter is a HARD failure emitting no numbers at all. See
`0403-*`. (2026-08-03)

**#404** — no schema for DECLARING a measured WCET. `MapperPath.exec_ms` is `Option<f64>` and nothing
outside rlm's own tests ever sets it, so rlm v0.1.4's `ChainFeasibleWithoutWcet` (issue 0259) now
reports missing evidence with nowhere to put it. Open questions: keying (board id / platform family /
named measurement profile — a WCET belongs to a context, not to code), unit (mapper wants ms, the bench
measures cycles, converting needs a clock rate), granularity (mapper wants a whole callback, the bench
measures primitives), and provenance/staleness. Blocked on 0403 — designing the schema before a
producer emits an artifact answers the keying question by guess. Invariant: absent stays representable
and stays the DEFAULT, else zeros get written in by hand. See `0404-*`. (2026-08-03)

Recently resolved (2026-08-10): **#415** — `nros::main!`'s framework table was deploy-keyed, so an
out-of-tree board declaring `framework = "embassy"`/`"rtic"` silently got a plain `fn main()` — an image
that links and then does not do what the framework was for. RESOLVED by phase-346 W1: the mapping moved
to `nros_orchestration_ir` (beside `board_path_for`), and an out-of-tree board reaches it through its own
manifest via `nros_build::emit_board_framework()` in the Entry package's build script. NOT at expansion
time as the issue proposed — a spike measured that proc-macro env/file reads are invisible to cargo's
fingerprint and that cargo-config values vanish when cargo runs from a workspace root, so that route
serves stale or empty answers, which is the same defect again. An unknown framework is now an ERROR
naming the accepted set. Proof it is reachable: `Framework::Embassy`'s `#[expect(dead_code)]` ("fires the
day it becomes constructible") went unfulfilled on build. See `archived/0415-*`. (2026-08-10)

RESOLVED 2026-08-05 — **#418** raw action feedback/result carried a SECOND CDR header, breaking raw↔{ROS 2, typed} decode. RFC-0069 (option A) made the producer header-less; the `nros-node` `payload_has_cdr_encap` value-sniff (a 2nd instance of #35) was deleted (splice unconditionally); C/C++/ffi audited clean. Verified: `action_envelope_tests` 3/3, a native typed-client ↔ Node-class server pair decodes result `[0,1,1]`, and `ros2 action send_goal --feedback` returns SUCCEEDED. 14/18 action cells green; the 4 remaining are blocked by build defects that predate this (freertos sizes-header / nuttx #0433), tracked under #0433 + #0422. See `archived/0418-*`.
(#416 resolved 2026-08-05 — `nros sync`'s source digest pruned build dirs by EXACT name, so it skipped
`target/` and walked straight INTO `target-tls` / `target-zenoh` / `target-xrce` / … , the isolated
build dirs `fixtures.toml` gives feature-variant rows. It then read every artifact it found, racing
cargo's own temporaries, and `just build-test-fixtures lane=native` died at a random fixture naming
an `rmeta`/`.o` path. RESOLVED by recognising a cargo build dir instead of listing names —
`CACHEDIR.TAG` plus a `target-` prefix for the not-yet-built case. Listing the dirs would have been
the hand-maintained-exclude-list shape issue 0287 already replaced. See `archived/0416-*`.)

Recently resolved (2026-08-07): **#455** — CLI unit tests hand-rolled 22 scratch-path spellings and the
differences between them WERE the bug: 6 keyed on a nanosecond stamp only (two processes collide), 3 on
a stamp with nothing else (same tick collides), 1 on nothing at all. Two shared a base name and raced —
one exec'd the stub `idlc` it wrote while the other truncated it (`Text file busy`), and the opening
`remove_dir_all` deleted the other's scratch. RESOLVED by one helper (`src/test_support.rs`): 26 call
sites across 19 files, and `git grep 'env::temp_dir' -- nros-cli-core/src` is now EMPTY — the ability to
spell it wrong is gone, not just the sites that had failed. Uniqueness no longer depends on the clock
(pid + process-wide atomic seq), and the pid segment is appended whether or not `CARGO_TARGET_TMPDIR`
is set — the earlier passes put it in the fallback only, so two concurrent runs of one INTEGRATION test
would still have shared a path. Two lessons recorded in the issue: its own residual table listed 9 sites
when the sweep finds 11 (third recurrence of the shape this issue documents), and the prescribed sweep
is line-based, so 8 of its hits are false positives on multi-line `format!(` calls that DO carry a pid.
Verified: 512 lib tests, three CONCURRENT suites clean, `check-cli-tests` 975 tests, clippy `-D warnings`.
See `archived/0455-*`.
**#454** — the two `*_send_goal_raw` FFIs (`nros-c` + `nros-cpp`) take a param named `goal_cdr` —
the same name their STRIPPING siblings use for `[CDR_HEADER][fields]` input — and pass it through
untouched, while every non-`_raw` sibling calls `strip_cdr_header`. `PollingActionClient::send_goal`
feeds `ffi_serialize` output (which carries a header) straight into one, so it would ship the #448
double encapsulation verbatim. LATENT, not live: `PollingActionClient` has no consumer anywhere and
neither `_raw` is called from `examples/` or `packages/testing/` — which is exactly why nothing
caught it. Found by sweeping the #448 class rather than by a failure. See `0454-*`. (2026-08-06)

Recently resolved (2026-08-07): **#453** — no native action cell could prove the goal payload was
DELIVERED: the cells asserted only `ACTION_RESULT_PREFIX`, which a client prints even when it decoded a
zeroed default result. That blind spot hid TWO real bugs in one week — **#448** (the client shipped two
CDR encapsulations, so Fast-DDS dropped every goal) and **#461** (the server decoded the goal UUID as
`order`, invisible with a nano-ros client whose UUID begins with a counter). A nano↔nano test cannot see
either, because both sides share the defect. Unblocked once all three example servers derived their
output from `goal.order` on ONE convention (order N → N+1 elements, matching ROS 2): #0450 fixed the
Rust server, the C server now stashes the order it already parsed instead of hard-coding 10, and the C++
loop moved from `i < order` to `i <= order`. The action rows now assert `FIBONACCI_ORDER_10_SEQUENCE`.
Verified by BREAKING it: a wrong expected sequence fails 9 of 18 cells — every action cell, 3 langs × 3
RMWs. See `archived/0453-*`.

Recently resolved (2026-08-06): **#448** — the Rust `send_goal` serialized with `new_with_header` and handed the
result to `send_goal_raw`, which frames the request itself — so every goal shipped TWO encapsulations
(`encap|uuid|encap|order` = 28 bytes vs ROS 2's 24). Fast-DDS sizes reader history from the type and
dropped the sample outright ("Change payload size of '28' bytes is larger than the history payload
size of '27'"), so the goal never reached the server and the client decoded a zeroed 12-byte default
result. Fixed by using the headerless `CdrWriter::new`, matching the RFC-0069/#0418 rule its siblings
`publish_feedback`/`complete_goal` already carried; `nros-c`/`nros-cpp` already stripped it, so the
Rust API was the lone live offender. See `archived/0448-*`. (2026-08-06)

Recently resolved (2026-08-11): **#470** — `large_msg::test_xrce_e2e_integrity` reported
`valid=false` on received samples — a payload-INTEGRITY verdict, so it read as corruption. It was
CROSS-TALK, and the failure output said so all along: **every 512-byte sample (this test's own) was
valid**; only foreign `size=64` ones were not. A second publisher's samples were landing in this
listener's subscription. TWO isolation leaks, both fixed. (1) The "unique" agent port was not unique —
`allocate_ephemeral_udp_port` bound port 0, read the number and CLOSED the socket, so the port belonged
to nobody until the agent bound it; measured 87 colliding ports in 2400 allocations across 12 processes.
Replaced by `nros_tests::port_lease`, a cross-process lock file held for the fixture's lifetime and
reclaimed when its pid is gone; the zenoh router's identical allocator was converted in the same change.
(2) All four XRCE stress tests shared the hardcoded topic `/stress_test` — distinct agents do NOT isolate
that, because an agent bridges its clients onto DDS and one host at one domain makes a shared topic a
shared bus. Added `STRESS_TOPIC`; each test names its own. **Ports alone did not fix it** — that is what
pointed past the transport. Both allocators' old unit tests asserted two SEQUENTIAL allocations differ,
which the racy allocator also satisfied (the kernel only re-hands a released port): guards that could not
fail on their own defect, now "distinct while HELD". Same wrong inference in `XRCE_LARGE_MSG_LOCK`, a
process-local `static Mutex` that serialises nothing across nextest's per-test processes. Verified 11/11
XRCE-clean ×3; tripwired. Never truly sweep-only — it reproduces in the `large_msg` binary alone, since
the colliding siblings live there. See `archived/0470-*`. (2026-08-11)

Recently resolved (2026-08-07): **#467** — `test_xrce_action_ros2_client` (the REVERSE of #448: nano is
the action SERVER, ROS 2 the client) rejected ~half the goals. The typed goal callback handed
`CallbackCtx` the WHOLE SendGoal request and `message::<M>()` skipped only the 4-byte encap, so the goal
type decoded its fields starting at the UUID. With a nano-ros client the UUID begins with a COUNTER, so
`order` always looked like a small positive number and the bug was invisible; with a ROS 2 client the
UUID is RANDOM, so `order` was a random i32 — negative half the time, hence the alternating
accept/reject. Already fixed upstream as **#0461**; verified here 3/3 solo runs pass against 3/3
failures before. Kept as a duplicate for the independent derivation. See `archived/0467-*`.

Recently resolved (2026-08-07): **#462** — `workspace_features` cell `rust/logging` counted 0 of an
expected ≥3 log lines carrying `[INFO]`. The defect was real — every hosted `log` bridge printed
`record.args()` bare — but it was already FIXED in source when this was filed: `f0fa793f4`
(nros-board-linux, the sink this cell exercises) is an ancestor of the filing commit, and `6863de1cc`
covered the other four sinks. What the issue captured was a STALE FIXTURE. The filed output settles
it: `nros: session open` is emitted by the BOARD bridge, not the node, and it too was untagged — so the
`native_entry` binary predated the fix. On rebuilt fixtures the same hand-run gives 8 of 8 marker lines
tagged, and all 17 cells pass. Note it did not present as `STALE`: the probe passed the binary through
and the assertion failed on its merits, reading as a live runtime defect (issue 0445's shape from the
other side). See `archived/0462-*`.

Recently resolved (2026-08-15): **#466** — tier 1's unstated, ORDERED setup contract. It accumulated four
defects over three months and all are now closed: the zephyr `skip_probe` freshness hole (`52e6bda8e`); the
precondition batch (one gate added, `check-artifact-identity-budget` checked and DECLINED because its
`started_at` filter already answers the case the finding cites, plus the launch-resolve skew now reported);
the compile-check gate being narrower than the tests it gates (phase-363 W4 — read the closure the build
MEASURED rather than guessing it); and tool-version drift (`nros setup --tool <name> --check`; `[tool.*]` was
the one declared class the doctor pass never walked, and `--check` swallowed the tool name). Closing that last
one surfaced two store layouts carrying two version vocabularies, both already declared in the index.
See `archived/0466-*`.

Recently resolved (2026-08-06): **#458** — `nros_cpp_executor_open_over_session` never stamped the `CppContext`
handle tag. The storage is `MaybeUninit`, so `cpp_ctx_checked` read garbage and every entry point
taking that handle returned `INVALID_ARGUMENT` (-3) — the generated per-tier setup's
`nros_cpp_node_create` failed, so `tier 'low' setup FAILED (rc=-3)` and `/telem` never published on
ALL THREE native C/C++ realtime cells. `tag` came from #0436, which stamped it in `nros_cpp_init` and
`_init_multi` and missed the THIRD constructor — the one every spawned tier uses. Same class as
#0387 (`in_dispatch`, from #0290) one field over, whose warning comment sits at the very same site.
See `archived/0458-*`. (2026-08-06)

Recently resolved (2026-08-06): **#447** — NOT a dead tier — multi-tier registration RACES on the shared RMW
session. Each spawned tier runs `setup` on its own thread while the boot tier runs the same closure
after the spawn loop, both declaring entities on one session unsynchronized (the board's
`SharedSession` comment claimed the backend serialized this; true for publish, false for
DECLARATION). Same binary, five runs: `telem 0`, then three crossed (`ctrl 0 / telem 1098` — the
10 ms stream on `/telem`, proven by `distinct=998` with duplicates from 10 up = two publishers), then
one correct. Fixed by serializing per-tier `setup` behind a mutex; 5/5 clean after. **A race is never
cleared by one green run** — a single passing rebuild here nearly got it misfiled as a stale fixture.
See `archived/0447-*`. (2026-08-06)

**#446** — the same crate is compiled ~21x across leaf target dirs: 106 `nros-core` rlibs, **5**
distinct `-C metadata` identities (45 of them the same compilation). Measured the exact
incompatibility factors — profile, feature set, RUSTFLAGS, and **explicit `--target` even for the
host triple** (so corrosion builds can never share with plain cargo builds). Separately,
`incremental = true` keeps the identity but destroys byte-reproducibility across dirs
(CGU session token), which blocks any content-addressed reuse; phase-336 made that profile the
default everywhere. Isolation is applied per-DIRECTORY while incompatibility lives per-IDENTITY, and
those are not the same partition. sccache's role is UNVERIFIED — measure before acting. See
`0446-*`. (2026-08-06)

Recently resolved (2026-08-07): **#422** — the runtime-E2E triage INDEX; it closes when its rows do,
and all four "remaining, untriaged" entries now have an answer. `test_ros2_action_xrce_client` was **#448**
(the Rust client shipped TWO CDR encapsulations, 28 B vs ROS 2's 24, so Fast-DDS dropped every goal);
`realtime_tiers` was **#447** (tier registration raced on the shared session) plus **#458**
(`open_over_session` never stamped the `CppContext` tag, so all three C/C++ tiers died with `-3`);
`native_example_reqresp`'s cpp/xrce/action cell passes; `logging_smoke` was a lane-coverage naming
problem, fixed there. Also spawned and resolved: #0427, #0428, #0429, #0438, #0441, #0461, #0462. One
correction carried forward: its "`large_msg::test_xrce_e2e_integrity` now PASSES" no longer holds — that
is now **#0470**. See `archived/0422-*`.

RESOLVED 2026-08-05 — **#431** NuttX cells skipped on a host that ran only `nros setup qemu-arm-nuttx`. (1) `NUTTX_DIR` is in fact exported by `sdk-env.sh` (verified clean-env) — the filing's claim was stale. (2) The real gap: no kconfig frontend, and the `pip install kconfiglib` remedy is refused on PEP-668 distros. `scripts/nuttx/build-nuttx.sh` now self-provisions kconfiglib into a repo-local venv (`build/nuttx-kconfig-venv`) when none is present — venv pip isn't PEP-668-blocked, no sudo. (3) `just nuttx doctor` already reports the state. So the cells now run instead of skipping. See `archived/0431-*`.


Recently resolved (2026-08-05 cycle) — #248, #382, #392, #398, #411, #412, #413, #416, #419, #420, #421, #423, #425, #426, #427, #428, #429, #430. Their summaries live in `docs/issues/archived/`.
