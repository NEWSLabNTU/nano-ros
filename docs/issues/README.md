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

**#481** — readiness greps use string LITERALS, so a wrong marker burns the whole timeout in silence and the
test still passes. Found by measurement: after phase-342 W1 split the pubsub fold, `rust_cyclone` sat at 34.1 s
against `cpp_cyclone`'s 5.2 s — 30 s timeout + 2 s settle + 2 s delivery — because the settle greped `"Waiting for"`,
which C/C++ print and the Rust listener never does (it prints `"Subscriber created for topic:"`). Both spellings were
ALREADY constants; the literal matched one language by luck. Fixed there (34.1 s -> 4.0 s, binary 95.1 -> 7.9 s), and
12 more call sites carry the same literal — 4 suspect (`executor.rs:138,200`, `esp32_emulator.rs:324,528`). NOT swept
blind: most wait on C/C++ binaries that do print it, and a wrong marker is silent, so each needs its own measurement.
Compounded by #0471 (a timeout returns `Ok`), so even a checked result would not notice. See `0481-*`. (2026-08-08)

**#480** (testing, open 2026-08-08) — the 29 `ci-matrix` test failures are NOT the QEMU-under-load flake
class: retested solo, **27 of 29 reproduce alone** and only 4 are flakes. Most share one cause — a test
waits for a banner its binary never prints (`native-rs-listener did not print \`Waiting for\``, while the
Rust listener prints `Subscriber created for topic:`). Issue 0471 documented exactly this, made the wait
STRICT so a missed banner fails instead of silently passing, and converted 4 suites — but **101 literal
`wait_for_output_pattern("…")` calls remain**, and the strictness is what turned them from falsely-green
into loud reds. A blanket replace is WRONG: `"Waiting for"` is correct for the C/C++ listeners and
service-server, wrong only for the Rust listener, so each site must be mapped to the binary it spawns.
Needs a gate forbidding string literals there — the rule has been in CLAUDE.md since phase-277 and is
violated 101 times.

**#480** (testing, open 2026-08-08) — **substantially duplicates #481, which is better evidenced — read
that one first.** Kept for the part it adds: a full audit of **101 literal `wait_for_output_pattern("…")`
calls** with each mapped to the binary it waits on. #481 found the same class by MEASUREMENT (a wrong
marker burns the whole timeout in silence: 34.1 s -> 4.0 s once fixed); this issue found it from a failing
test and carries the site table. **CORRECTED:** #480 originally claimed this explained the 29 ci-matrix
reds. It does not. Re-running all 27 with full capture shows fixture coverage/staleness dominates (10 "not
prebuilt", 3 "failed to build", 2 STALE), and the bare-`cargo nextest` retest miscounted `skip!` panics as
failures — the caveat CLAUDE.md states outright. Both issues agree a blanket replace is WRONG: most sites
wait on C/C++ binaries that DO print `"Waiting for"`.

**#479** (examples, open 2026-08-08) — `5f4eda8a4` fixed issue 0453 (an action server whose output ignored
the goal payload) on the **native** cells only; all 8 embedded copies still carry the old body, so the
defect 0453 was filed about is live on 8 of 10 cells. Two divergences: the four embedded C++ copies run
`i < goal.order` where native runs `i <= goal.order` (an order-N goal yields N elements, siblings yield
N+1), and the four embedded C copies never gained the `accepted_order` slot, so the bound is right and the
input is still dropped. Caught by `example_portability copies_within_a_group_are_identical` — one of the
few `ci-matrix` failures that is not a QEMU flake; reproduces solo in 0.4 s. NOT propagated blindly: the
nuttx C copy has `(void)context;` where native casts it to `server_context_t*`, so the embedded cells may
surface the callback context differently and a paste could break four platforms.

**#478** (build, RESOLVED 2026-08-08) — cc-rs sent `-mno-omit-leaf-frame-pointer`, a **clang** flag, to
`arm-none-eabi-gcc`, which rejects it — every `freertos` fixture row died `rc=101` while the other six
modules passed, and it was the last gate on `just ci-matrix`. Nothing in-tree passed it: cc-rs adds it when
forcing a frame pointer off `debug = 1`, and it arrived with no commit behind it because the lock pinning
`cc` here is the mixed workspace's, generated per host and untracked. Fix: `gcc_safe_frame_pointer` drops
cc-rs's automatic pair and re-adds the half gcc understands (clang/MSVC untouched), called from INSIDE
`strict_decls` — the one function every nano-ros C compile already calls — so ~20 sites were fixed with no
call-site edits; 7 unrouted sites in the two freertos board crates name it directly. Gated by
`check-cc-build-policy`, tripwired both ways. Verified: `lane=tier2` all eight modules OK.

**#477** (nuttx, RESOLVED 2026-08-08) — `nuttx-c-talker-zenoh` overflowed ROM by 448776 bytes and gated
`lane=all`/`lane=tier2`. **Not a size regression at all.** `nros-board-common`'s `snapshot_root` /
`snapshot_or_tree` prefer the per-arch NuttX export snapshot and fall back to the live `staging/` tree, but
emitted `cargo:rerun-if-changed` on the path that WON — so a build that ran before the ARM export existed
pinned its edge to `staging/`, and the snapshot later appearing changed nothing it watched. A `lane=all`
sweep builds RISC-V and ARM, so the board artifact stayed staged against the wrong tree. Fix: emit the edge
on the preferred path even on the losing branch (a `rerun-if-changed` on a missing path still fires when it
APPEARS). Proof: after provisioning + `cargo clean` of the two crates, HEAD linked with the same code —
691468 bytes vs 687112 on Aug 6, +0.6 %; the ARM lane now completes with zero overflows. The bisect never
took a step: the confounder surfaced while validating the endpoints, which is why both ends get validated
first.

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

**#472** (build, open 2026-08-06) — thirteen of fifteen opaque-storage macros have NO compile-time size
check. C/C++ callers allocate opaque byte arrays sized from Rust's `size_of::<T>()`; only
`EXECUTOR_OPAQUE_U64S` and `CPP_EXECUTOR_OPAQUE_U64S` assert the type fits. For PUBLISHER, SUBSCRIPTION,
SESSION, SERVICE_*, ACTION_*, LIFECYCLE_CTX and the RAW_* set, a too-small value is a short buffer at
runtime rather than a build error. Split out of #464: that issue removed two SOURCES of wrong sizes (a
poll that could return another consumer's rlib; constants rotted ~11% low) and is done, but the guards
outlive it — they are what makes any future wrong size fail instead of corrupt. Also here: nros-cpp's
"probe returned 0 -> OPAQUE_U64S = 1, do not link" is advice in a build-script warning with nothing
enforcing it; issue 0360's variant-symbol mechanism would make it a link error. Fix must be generated +
gated, not fifteen hand-written asserts — the thirteen are unguarded because each was added one at a time.

**#471** — `wait_for_output_pattern` returns `Ok` on TIMEOUT whenever the process printed anything at all;
the pattern is consulted only for the early-exit path. So `wait_for_output_pattern(MARKER, …)?` means "the
process was not silent", NOT "the marker appeared". **233 of 283 call sites** ignore the returned string and
check only the `Result`. RESOLVED (2026-08-07): the contract is now strict — `Ok` means the pattern appeared,
`Err` quotes the output — with `collect_until()` as the lenient counterpart under an honest name; both share
one `(String, bool)` engine, since conflating "what was printed" with "did it match" WAS the defect. The same
two lenient paths existed in `QemuProcess` and were fixed with it. The flip caught exactly one class, 15-16
tests: suites waiting for the literal `"Waiting for"`, a banner `examples/native/rust/listener` stopped
printing at phase-277 — now `output::LISTENER_READY_MARKER`. Those suites also got 2-3x faster, having been
burning a full 5 s timeout per listener. See `0471-*`. (2026-08-07)

**#476** RESOLVED (2026-08-07): writing an executable stub and exec'ing it races against sibling test
threads — `O_CLOEXEC` closes a descriptor at EXEC, not at FORK, so any concurrent `Command::spawn` inherits
the still-open write handle and our `execve` gets `ETXTBSY`. **Unique paths do not fix it** (that was #455's
cause, and it was already fixed here — the failing path was pid-scoped). Measured 245/1200 execs failing at
12 forking threads. Fix: `test_support::write_executable_stub` writes via CHILD `cp`+`chmod`, so no write
descriptor ever exists in our process — 0/1200. See `0476-*`. (2026-08-07)

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
**#464** (build, open 2026-08-06) — the size probe has THREE stacked fallbacks and the last one is a
stale literal. `nros-c`/`nros-cpp` derive the C/C++ opaque-storage macros from Rust's `size_of::<T>()`:
(1) an isolated nested cargo build, (2) a **poll of the outer target dir** with a 60 s timeout, (3)
committed `NUTTX_FALLBACK_SIZES`. Each masks the previous, and all three transitions announce
themselves only via `cargo:warning`, invisible in a normal build. Layer 3's `EXECUTOR_SIZE = 79_296`
sits BELOW the measured `89_392`, and these macros size the byte arrays C/C++ callers allocate for Rust
types — so losing a timing race substitutes a short buffer. The reassurance that "the const assertion
catches it" holds for **2 of 15** opaque macros (`EXECUTOR`, `CPP_EXECUTOR`); the other thirteen —
PUBLISHER, SUBSCRIPTION, SESSION, SERVICE_*, ACTION_*, LIFECYCLE_CTX, the RAW_* set — have no
compile-time size check at all. Layers 1+2 exist to cover each other: layer 2 needs ordering, which is
why `nros` is a build-dependency purely to force it (phase-340 W5.a: 16 units and 4 duplicated crates
per invocation), and layer 1 needs no ordering, making that edge dead weight for the default path. The
fallback's stated justification (custom JSON target specs) is stale — none exist in the tree, and NuttX
build-std is now handled inside layer 1. PROBE HALF FIXED 2026-08-06/07 (`8e3bfc639`): both fallbacks deleted, the probe now computes or fails,
and `just verify-size-probe` — which had itself been exiting 1 before asserting anything, its HEADER
pointing at a stub with zero size defines — was resurrected. Verified on NuttX, the target the constants
existed for: the probe's stamp names an isolated nested build for `riscv32imac-unknown-nuttx-elf` and
yields 88224, ~11% ABOVE the deleted 79_296. STILL OPEN, and the larger risk: the thirteen unguarded
opaque macros, so any future wrong size still lands as a short buffer rather than a build error.

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

**#460** — RE-MEASURED 2026-08-07 on fully fresh fixtures: **half fixed, and the shape changed.**
`nuttx-arm/rust/entry_pubsub` now PASSES (not attributed). `zephyr/rust/params` still fails — and so do
`zephyr/rust/lifecycle` and `zephyr/rust/qos`: `12 ran, 0 skipped, 3 failed (of 15)`. Three cells, one
platform+language, three different FEATURE entries — a family, so the suspect is the zephyr-rust
entry's feature wiring rather than three unrelated runtime paths. The measurement itself was the
obstacle: the first re-run reported "1 of 15 FAILED" listing neither cell, which reads as "both fixed",
because the harness reported skips ONLY when every cell skipped — ten passes and four skips printed
identically to fourteen passes. Now always prints `N ran, M skipped, K failed` with the list (fixed in
`463748763`). Rebuilding zephyr also needed `nros sync` in the seven zephyr rust leaves first, per
#0463. See `0460-*`. (2026-08-07)

**#451** — the embedded SDK env vars live only in `just/sdk-env.just`, so a direct `cargo build` of an
embedded example fails one variable at a time (5 freertos, 4 threadx, 1+ nuttx) even though every SDK sits
at the default path. The failure does not look like a missing variable: `zpico-sys` panics inside a
dependency build script, and a partially-configured NuttX build reaches the LINKER with `undefined
reference to open / socket / malloc` — mistaken for a link regression during phase-338. CLAUDE.md names
`activate.sh` the env SSoT; for these it is not. Same shape as 0407 and 0420's real finding.
See `0451-*`. (2026-08-06)

**#452** — embedded builds regenerate `nros_generated.h` / `nros_cpp_ffi.h` with an OLDER cbindgen, so any
embedded lane silently dirties two tracked headers, and committing it REVERTS the C23 enum-base guard (had
to be hand-reverted twice during phase-338). The repo already pins `clang-format` and `bindgen-cli 0.72.1`
for the C→Rust direction for exactly this reason; the Rust→C direction has no pin.
See `0452-*`. (2026-08-06)

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

**#432** — the pinned `zephyr-lang-rust` (404fcef) cannot build the `zephyr` crate for ANY board
whose devicetree has gpio nodes: its DT generator emits a five-argument `GpioPin::new` against a
six-argument signature (`pin` without `dt_flags`). `CONFIG_GPIO=n` makes it worse, not better — the
generator reads the devicetree, and the `gpio-keys` augment carries no `cfg:` key, so the calls are
still emitted while the `raw` bindings vanish (14 errors instead of 4). Invisible until phase-337
W2.b added the first non-native_sim Zephyr board, native_sim having no gpio nodes. Since essentially
every real board has gpio, Rust-on-Zephyr is native_sim-only until this is fixed upstream; C and C++
are unaffected (no `zephyr` crate), which is why W2.b's cells build the C entry. See `0432-*`.
(2026-08-05)

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

**#371** [severity **high**] — native_sim cyclone app abort()s at a near-deterministic 19–21 s
joining the full Autoware graph during the safety-island demo's scenario init (7/7 on 2026-08-01;
the same tree passed 2× on 2026-07-31 when one sim node was down). mrm_handler flaps
operate/cancel (hot service path) right before death; unnamed cyclone pthread; gdb masks it;
strace -k unwinds only to the zephyr print shim. Isolation: island alone / single-peer feed /
availability flap / idle sim / EKF-odometry all survive — only the full graph + scenario churn
kills it. TRIGGER CONFIRMED by A/B: shadowing autoware_manual_lane_change_handler out of the sim flips 7/7 aborts to VERDICT PASS. See `0371-*`. (2026-08-01)

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

**#415** — `nros::main!` picks the framework emit shape from a hardcoded **deploy-string** table,
while `nros ws check` reads `[package.metadata.nros.board] framework` off the board crate. Two
spellings of one fact, and the macro's falls through to `OwnedSpin` on an unknown key — a silently
wrong entry shape, not a diagnostic. Invisible until phase-337 W7.a deleted `embassy-stm32f4`, the
only in-tree key that selected `Framework::Embassy`; `Framework::Rtic` is in the same shape and only
still reachable via `rtic-mps2-an385`. Fix = let the macro read the same manifest key (needs the
expansion-time fs round-trip `rtic_board_spec_for` already defers). See `0415-*`. (2026-08-04)

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

**#470** — `large_msg::test_xrce_e2e_integrity` fails ONLY inside the full sweep, and fails as
`valid=false` on every received sample — a payload-INTEGRITY verdict, not a timeout or a missing
message. Passes solo every time (2/2, ~5 s), so by the retest-solo rule it reads "load-sensitive". But
load normally shows up as ABSENCE, not as delivered-but-corrupt, so either the CHECK is racy (shared
XRCE agent / expected-pattern source crossed with a concurrent test) or the data really is corrupt
under concurrency and solo runs never apply the pressure. Nothing so far separates those. First step:
re-run the sweep with this test's agent isolated via the `nros_tests::alloc` allocator. #0422 had
recorded it as "now PASSES". See `0470-*`. (2026-08-07)

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

**#466** — Tier 1 has an unstated, ORDERED setup contract. Eight consecutive attempts at `just ci` on a
clean, provisioned tree each stopped on a DIFFERENT precondition, and only one stop was a test: missing
GNU `parallel` (the lane degrades to serial and reads as a hang), a stale in-tree `nros` after a tree
refresh, `nros_box_publish` reading an intentionally-unset `$CARGO_TARGET_DIR`, then four source-level
reds already sitting on main — including a crate that did not COMPILE (`mod log_bridge;` committed
without its file). Two halves: (A) the required sequence — `nros setup --system`, CLI rebuild, publish,
resolver, lane-scoped fixtures — is documented only in pieces, is order-dependent, and is re-armed by ANY
refresh (pull/rebase/stash/rsync restages the CLI stamp and every fixture mtime); (B) those gates run
ONLY in `just ci`, so while it is unreachable, reds accumulate — measurably, since two of the four were
being fixed by a concurrent session in the same hour and landed as duplicate patches. **PARTLY FIXED
2026-08-07:** the fixture-free lane was never missing — `check-fast` IS it, and had been unreachable
because `check-cli-fresh` (needs a fresh CLI) and `check-test-targets` (needs the `-sys` sources)
both sat in it, contradicting its own "buildless and SOURCE-FREE" docstring. With those moved to
`check-build`, plus `check-leaf-lockfiles` no longer treating an unsynced tree as a failure, per-push
CI went green after **20+ consecutive red runs** — and a third red behind them, `scaffold-journey`
asking `nros new` for a platform it has refused since 2026-07-28, only became visible once the first
two cleared. That sequence is the issue's own thesis observed rather than argued: a permanent red does
not fail, it hides its neighbours. Still open: proposal (1), one precondition gate reporting every
unmet item at once — worth more than first argued, since `just` stops at the first failed dependency
and 25 gates sat behind `check-test-targets` alone. See `0466-*`. (2026-08-07)

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
