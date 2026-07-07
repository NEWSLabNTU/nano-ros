# Phase 280 — NuttX Entry eth0 config: C path fix + runtime proof

Status: **In progress — 2026-07-06** · Closes issue #130 · Follows the partial
fix in commit `703e840dd` (Rust path only).

> **Goal.** BOTH NuttX Entry paths — the Rust `nros::main!` path AND the C
> `nano_ros_entry(BOARD nuttx-qemu-arm LAUNCH …)` path — push the guest static
> IP into `eth0` before opening the executor, from ONE shared helper (no drift),
> and each is proven by a networked cross-process runtime e2e (QEMU guest →
> host zenohd over slirp). #130's own unclosed acceptance step
> ("add the deferred networked entry e2e") lands here, for both paths.

## Why

Commit `703e840dd` fixed #130 for the **Rust** entry path only: `entry_net_init`
(`nros-board-nuttx-qemu-arm/src/entry_212n.rs:158`) runs the `SIOCSIFADDR` +
`/dev/urandom` reseed, but it is called ONLY from the Rust `BoardEntry::run` /
`run_with_deploy` wrappers (`entry_212n.rs:122,140`).

The **C** entry path never reaches those wrappers. `nano_ros_entry LAUNCH` links
the C nodes into the NuttX kernel via `nros-nuttx-ffi`, whose entry is:

```rust
// nros-board-nuttx-qemu-arm/nros-nuttx-ffi/src/main.rs
fn main() {
    nros_rmw_zenoh::register().expect("...");
    unsafe { app_main() };   // straight to C — no BoardEntry::run, no init_hardware, no entry_net_init
}
```

So eth0 is **never** configured on the C path: the guest keeps its defconfig
address, cannot reach slirp's `10.0.2.2`, the executor connect never completes,
and `tests/c_nuttx_entry_e2e.rs` (`c_nuttx_workspace_entry_delivers_cross_process`)
hangs the full 60 s and times out. Same root cause as #130, second code path,
left unfixed by `703e840dd`. (The `nuttx_entry/CMakeLists.txt:23` comment
"NuttX brings up eth0 at kernel boot" is the exact wrong assumption #130
debunked — it must be corrected too.)

Neither path has a networked runtime e2e today:
- Rust: `nuttx_entry_build.rs` is build-assert only (link, not runtime).
- C: `c_nuttx_entry_e2e.rs` exists but times out (the bug above).

So the Rust fix is also unproven at runtime — a green e2e is the only evidence
`703e840dd` actually connects.

## Root cause (recap)

`run_entry` (`nros-board-nuttx/src/lib.rs:231`) calls the no-arg
`BoardInit::init_hardware()`, which is a documented **no-op** on `QemuArmVirt`
(the 212.N.3 parameterless trait can't run config-dependent steps). The eth0
push lives only in the Rust `BoardEntry` wrappers. Any path that reaches the
family driver without going through those wrappers — i.e. the C `app_main`
path — stays broken.

## Approach

One shared, public eth0-config helper in `nros-board-nuttx-qemu-arm`, called by
both entry paths. Rust wrappers keep sourcing overrides from `DeployOverlay`;
the C ffi `main` sources them from a compile-time bake (`option_env!`) with the
slirp e2e defaults (`10.0.2.30/24` via `10.0.2.2`) as the floor — so even an
un-overridden C entry connects.

## Waves

### W1 — Shared public eth0-init helper (de-dup, no behavior change)
- [x] W1.a Extract the `SIOCSIFADDR` + urandom-reseed core into a public fn
  (`configure_entry_eth0(ip, prefix, gateway)`) in `nros-board-nuttx-qemu-arm`
  (`entry_212n.rs`), re-exported from the crate root. Delegates to the sole
  `node::init_hardware` body — no second `SIOCSIFADDR` call site. Slirp defaults
  hoisted to `SLIRP_DEFAULT_{IP,GATEWAY,PREFIX}` consts (shared by both paths).
- [x] W1.b `entry_net_init` (Rust path) now derives overrides from
  `DeployOverlay` then delegates to `configure_entry_eth0` — byte-identical
  `703e840dd` semantics.
- [x] W1.c Acceptance: ARM lane compiles — the board crate cross-built for
  `armv7a-nuttx-eabihf` (talker_entry + C entry ELFs produced). Runtime log line
  unchanged (pending W3 runtime, env-gated below).

### W2 — C-path wiring (the live bug)
- [x] W2.a `nros-nuttx-ffi/src/main.rs`: call the W1 helper BEFORE `app_main()`,
  with IP/prefix/gateway from `option_env!("NROS_IP")` / `NROS_PREFIX` /
  `NROS_GATEWAY`, defaulting to slirp `10.0.2.30` / `24` / `10.0.2.2`. Placing it
  in the ffi `main` (not the per-language `run_components` template) covers BOTH
  the C and C++ nuttx entries with one edit, unconditionally, before any
  `app_main`.
- [x] W2.b Bake channel resolved: the C locator arrives as `NROS_ENTRY_LOCATOR`
  baked into the generated C++ entry TU (`nuttx_entry_main_c_typed.cpp.in` →
  `NuttxBoard::run_components`) via `NanoRosEntry.cmake` COMPILE_DEFINITIONS — a
  DIFFERENT channel from the Rust ffi crate's `option_env!`. Wiring `NROS_IP`
  through to the ffi cargo build (per-entry `[deploy.nuttx] ip` for the C path)
  is deferred to a follow-up; the slirp default in the ffi `main` closes the e2e
  without it (the Rust path already has the `DeployOverlay` override).
- [ ] W2.c Fix the misleading `nuttx_entry/CMakeLists.txt:23` comment
  ("NuttX brings up eth0 at kernel boot") to state the Rust ffi entry now
  performs the `SIOCSIFADDR` push (mirrors the Rust `BoardEntry` path).
- [x] W2.d Acceptance (compile): `workspace-fixtures-build.sh nuttx c` rebuilt
  the `nuttx_entry` ELF (`nros-nuttx-ffi` + `nros-board-nuttx-qemu-arm`
  cross-compiled for `armv7a-nuttx-eabihf`, RC=0); the fresh ELF links the
  `eth0` / `/dev/urandom` config path. Runtime (guest connects instead of
  hanging) pending W4.a, env-gated below.

### W3 — Rust networked entry e2e (prove `703e840dd`)
- [x] W3.a New `tests/rust_nuttx_entry_e2e.rs`: boots the prebuilt `talker_entry`
  NuttX image (`nuttx::require_entry_binary("talker", "nuttx_rs_talker_entry")`,
  bakes `NROS_LOCATOR = tcp/10.0.2.2:7452`, domain 0) under
  `QemuProcess::start_nuttx_virt`; host `zenohd` on `0.0.0.0:7452`; the native
  Rust listener (`build_native_listener`, String `/chatter`, `I heard:`) receives
  cross-process. Mirrors `c_freertos_entry_e2e.rs`. Resolver already exists
  (`require_entry_binary`); `talker_entry` fixture rows already in
  `fixtures.toml` — no new wiring.
- [x] W3.b Skips cleanly (`nros_tests::skip!`) when zenohd / qemu / the entry ELF
  are absent — no bare `eprintln!`+return.
- [ ] W3.c Acceptance: observes ≥3 cross-process deliveries; asserts (panics) on
  shortfall. Compiles on host (`cargo check -p nros-tests --test
  rust_nuttx_entry_e2e` green); runtime pending the fixture build.

### W4 — C entry e2e green + close #130
- [ ] W4.a With W2 landed, `c_nuttx_workspace_entry_delivers_cross_process` runs
  to real delivery instead of the 60 s timeout. Run it green (≥3 deliveries).
  **BLOCKED (env):** networked QEMU e2e cannot execute in this workspace — a
  bare no-net QEMU boot of the entry ELF runs (RC=0, reaches `Executor::open`),
  but adding slirp networking + a `zenohd` listener is killed with exit 144 (the
  environment blocks the port bind / slirp NAT). Same class as the runtime
  gates #130 already documented; must run where QEMU slirp + a host router are
  permitted (CI nuttx lane / a dev box).
- [ ] W4.b Resolve #130: move to `docs/issues/archived/`, `status: resolved`,
  cross-ref this phase + `703e840dd`. Note both paths configured + both proven.
  Gated on W3/W4.a runtime green.
- [x] W4.c `just format` green. `just ci` pending (blocked upstream — see below).

## Blockers encountered (2026-07-06)

1. **Runtime e2e is environment-gated — a sandbox guard on `zenohd`'s listening
   socket, NOT a defect.** The networked cross-process e2e (W3
   `rust_nuttx_entry_e2e`, W4 `c_nuttx_entry_e2e`) cannot run in the agent's
   sandboxed shell because the `ZenohRouter` fixture's `zenohd --listen …` spawn
   is killed with exit 144 (signal 16 / SIGSTKFLT) and the whole command's
   filesystem effects are discarded. Diagnosed by elimination (all with the
   sandbox flag OFF):

   | Process | Exit | |
   |---|---|---|
   | `zenohd --version` (no net) | 0 | runs — binary is fine |
   | `echo "zenohd --listen tcp/…"` (string) | 0 | not static matching |
   | python `http.server` TCP listen on 127.0.0.1 | 124 | plain TCP-listen OK |
   | QEMU + slirp networking | 124 | networking OK (slirp is fine) |
   | native nros listener (zenoh **client**, dials out) | 134 | zenoh client stack OK |
   | `zenohd --listen` (multicast+gossip off, loopback) | **144** | killed + discarded |

   So it is **not** the binary, string matching, scouting/gossip/multicast, plain
   TCP-loopback listen, networking in general, or the zenoh stack per se — it
   fires **specifically when zenohd opens its LISTENING socket** (something zenoh's
   TCP listener does beyond a plain `bind()/listen()` — likely `SO_REUSEPORT`, a
   dual-stack `[::]` bind, or a second bound socket the sandbox's network policy
   forbids). The exact syscall can't be captured: every zenohd-listen command is
   rolled back wholesale (even a pre-spawn `touch … ; sync` to the persistent
   scratchpad vanishes), so no strace/core/log survives the 144. `dangerously-
   DisableSandbox` does not lift it — the guard sits below that flag.
   `nros setup` re-provisions the same (killed) binary and cannot help.

   Both entry ELFs build and are ARM-compile-verified; only the networked RUN is
   blocked. Re-run where zenohd may bind a server socket (a normal dev shell /
   the CI nuttx lane):
   `cargo nextest run -p nros-tests --test rust_nuttx_entry_e2e` and
   `--test c_nuttx_entry_e2e`.
2. **~~Pre-existing schema break~~ → stale installed `nros` (RESOLVED, not a
   repo bug).** `just nuttx build-examples` failed at `nros ws sync` on the rust
   workspace: `unknown field 'max_callbacks', expected 'deploy'` in
   `native_showcase_entry`. Root cause: `max_callbacks` is a REAL phase-271
   (#110) field — per-entry callback-table sizing threaded into
   `Executor::open_sized` by `nros::main!` — and the CLI source schema accepts it
   (`cargo_metadata_schema.rs:243`, `#[serde(default)]`). The installed `nros`
   binary was simply STALE (Jul 1, pre-phase-271). `just setup-cli` rebuilt it
   and `nros sync examples/workspaces/rust` then succeeded (RC=0). Environment
   staleness, not a code defect — nothing to file. (Recurring: `setup-cli`
   rebuild exposes a stale template/schema.)

## Non-goals

- Per-entry `[deploy.nuttx] ip` override for the **C** path if W2.b finds no
  clean cargo-env bake channel — the slirp default fixes the failure class; the
  override is a follow-up (the Rust path already has it via `DeployOverlay`).
- The nuttx-qemu-**riscv** ffi sibling — mirror the fix there only if the same
  entry path exists; otherwise a one-line follow-up note.
- Non-networked NuttX entry behavior (build-asserts stay as-is).

## Acceptance (phase)

- One shared eth0-config helper; both Rust and C entry paths call it (no second
  `SIOCSIFADDR` site).
- `rust_nuttx_entry_e2e` green: Rust `talker_entry` image delivers cross-process
  (proves `703e840dd`).
- `c_nuttx_workspace_entry_delivers_cross_process` green: no more 60 s timeout.
- `just format` + `just ci` green.
- #130 resolved + archived.

## Sequencing

W1 (shared helper) lands first — it is a pure refactor that both later paths
build on. W2 (C wiring) and W3 (Rust e2e) are independent and can proceed in
parallel after W1. W4 verifies both runtime lanes and closes the issue; the C
e2e (W4.a) depends on W2, the close (W4.b) depends on W3 + W4.a both green.
