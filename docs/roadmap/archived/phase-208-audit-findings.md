# Phase 208 Stage 2 — Book Starter Tutorial Audit Summary

Synthesis of 14 strict-follow execution-agent reports under
`tmp/book-audit/reports/`. Worktrees preserved at
`.claude/worktrees/agent-<id>/` for forensic inspection.

## Verdict

**Every Linux-first and embedded starter has at least one BLOCKER that
prevents a strict-follow new user from reaching `Published: N`.** The
recurring blockers are environmental (env vars, submodule fetch, PATH
hygiene) and schema (`config.toml` vs the actual `nros.toml`). Per-page
bugs are layered on top.

Stage 0's 8 spot fixes were necessary but nowhere near sufficient.

## Severity matrix

| Tutorial | Blockers | Friction | Clarity | Verdict |
|---|---:|---:|---:|---|
| `installation.md` | 2 | 3 | 3 | **broken** — Pattern B build fails |
| `first-node-rust.md` | 1 | 1 | 4 | **broken** — `cargo build` panics |
| `first-node-c.md` | 3 | 3 | 1 | **broken** — build never reaches binary |
| `first-node-cpp.md` | 3 | 3 | 0 | **broken** — env + submodule + `LANGUAGES` |
| `troubleshooting-first-10-min.md` | 0 | 2 | 2 | **degraded** — symptom-1 fix misleading |
| `freertos.md` | 3 | 2 | 3 | **broken** — schema + QEMU `-nic` + workspace dep |
| `threadx.md` | 5 | 3 | 1 | **broken** — schema, `tap-tx0`, QEMU cmd, link |
| `bare-metal.md` | 2 | 5 | 1 | **broken** — env + schema + invented runner flags |
| `integration-nuttx.md` | 5 | 3 | 2 | **broken** — fake NSH cmd, bad QEMU flags |
| `integration-zephyr.md` | 3 | 3 | 3 | **broken** — `nros-codegen` legacy, Kconfig symbols |
| `esp32.md` | 4 | 2 | 2 | **broken** — `just esp32 build` no-op, schema |
| `integration-esp-idf.md` | 1 | 0 | 0 | **broken** — `REQUIRES nano-ros` fails to resolve |
| `integration-platformio.md` | 1 | 3 | 0 | **broken** — `.ini` doesn't build, invented macros |
| `px4.md` | 0 | 3 | 1 | **degraded** — setup half-applies, doc bug `-D` vs env |

## Cross-tutorial recurring patterns

Numbered for cross-reference. Each pattern lists every tutorial that
hit it.

### P1 — `NROS_PLATFORM_CFFI_INCLUDE` env-var not exported

`zpico-sys/build.rs:1623` panics on fresh shell: `NROS_PLATFORM_CFFI_INCLUDE
not set (direnv allow, or build via just)`. The var is set only by
`just/sdk-env.just`; the repo `.envrc` does NOT export it. The panic's
own hint is misleading — `direnv allow` does nothing here.

**Hit by:** installation, first-node-{rust,c,cpp}, troubleshoot,
freertos, bare-metal, esp32, threadx (sibling `NROS_PLATFORM_THREADX_SRC`).
**Fix candidates:** (a) export from `.envrc`; (b) `build.rs` autoresolves
the path under `~/.nros/sdk` or `<repo>/packages/zpico/...`; (c) every
tutorial must say "run via `just`, never bare `cargo build`".

### P2 — Filename + schema drift: `config.toml` → `nros.toml`

Every embedded tutorial shows a `config.toml` snippet with
`[network]/[zenoh]/[scheduling]/[wifi]` tables and `prefix = 24`. The
actual file is `nros.toml`, the schema is `[node]/[[transport]]/[node.rt]`,
addresses are CIDR strings (`ip = "10.0.2.10/24"`), and `locator` lives
inside `[[transport]]`. Copy-paste produces a file the loader ignores.

**Hit by:** freertos, threadx, bare-metal, esp32 (and the implicit
references in nuttx, zephyr, esp-idf, platformio).
**Fix:** rewrite every Configure section against the real schema; grep
the rest of `book/` for `config.toml` and re-check.

### P3 — `px4-rs` submodule not fetched by `nros setup native --rmw zenoh`

Workspace member `nros-tests` depends on `px4-sitl-tests` at
`third-party/px4/px4-rs/tests/sitl`. `nros setup native --rmw zenoh`
doesn't fetch it. Workaround: `nros setup --source px4-rs`.

**Hit by:** installation, first-node-{rust,c,cpp}, freertos.
**Fix candidates:** (a) gate the workspace dep behind a feature off by
default; (b) include `px4-rs` in the native package plan; (c) doc adds
the second `nros setup --source` step.

### P4 — `zenohd` not on PATH after `nros setup … --rmw zenoh`

Binary at `~/.nros/sdk/zenohd/<v>/bin/zenohd`; doc tells user to run
`zenohd`; not on PATH. `~/.nros/bin/` only has `nros`.

**Hit by:** first-node-c, first-node-cpp, bare-metal (implicit),
installation (workaround listed as fragile one-liner).
**Fix:** `install-nros.sh` / `nros setup --rmw zenoh` symlinks zenohd
into `~/.nros/bin/`.

### P5 — Doc CMake snippets diverge from canonical example

Doc shows `NANO_ROS_RMW` literal + explicit `nano_ros_link_rmw(target
RMW zenoh)`. Real examples use `NROS_RMW` cache var (forwarded to
`NANO_ROS_RMW` inside the example) and register transitively via
`nros_platform_link_app`. Both work, but a user diffing the doc against
the GitHub source link is confused.

Worse: `first-node-cpp.md` says `project(my_talker LANGUAGES CXX)` —
missing `C` causes link fail when the register-stub C TU is generated.

**Hit by:** first-node-c, first-node-cpp, installation (Pattern B).
**Fix:** align doc with canonical example shape (drop `nano_ros_link_rmw`
on POSIX; rely on `nros_platform_link_app`'s transitive registration).
Always `LANGUAGES C CXX`.

### P6 — Embedded host daemon not started

Doc doesn't tell users to launch `zenohd -l tcp/127.0.0.1:<port>` before
booting QEMU. Per-platform ports are 7450 (bare-metal), 7451 (FreeRTOS),
7453 (threadx-riscv64), 7454 (ESP32), 7455 (threadx-linux), 7456 (Zephyr).
The host's default zenohd on 7447 doesn't match, so the talker fails
`Transport(ConnectionFailed)` and exits without `Published:`.

**Hit by:** esp32 (most acute — talker boots, fails, doc silent), freertos
(7451 mentioned but no start step), threadx, bare-metal (7450 fix in
Stage 0).
**Fix:** every embedded tutorial gets a "Run: terminal 1 — start
`zenohd -l tcp/127.0.0.1:<port>`" step. Or: provide per-platform
`just <plat> zenohd` recipes and have the doc call them.

### P7 — Output banner / `Published: 0` off-by-one

Doc claims `Published: 1` first; actual first index is `0`. Bare-metal
banner is `nros QEMU Platform` not `nros Bare-Metal Cortex-M3 Talker`.

**Hit by:** bare-metal, first-node-rust, freertos.
**Fix:** s/Published: 1/Published: 0/ across all tutorials; cite the
real banner from `src/main.{rs,c,cpp}`.

### P8 — QEMU invocation drift

Doc QEMU commands diverge from the canonical `just` runners. Examples:
- `freertos.md` uses `-nic socket,model=lan9118,listen=:6666` (bridge);
  Slirp IPs in `nros.toml` need `-nic user,model=lan9118`.
- `bare-metal.md` claims runner sets `-nic`; it doesn't.
- `nuttx.md` says `-cpu cortex-a8`; canonical is `cortex-a7`. Missing
  `-netdev user,id=net0 -device virtio-net-device,netdev=net0`.
- `threadx.md` `-kernel ./build/talker.elf`; real binary lives at
  `target-zenoh/<triple>/<profile>/qemu-riscv64-threadx-talker` (no
  `.elf`). Missing `-bios none -global virtio-mmio.force-legacy=false`.

**Hit by:** freertos, threadx, bare-metal, nuttx.
**Fix:** every embedded tutorial uses `just <plat> talker` (the canonical
runner) for the happy path. Direct `qemu-system-*` invocation, if shown
at all, is sourced from the recipe's actual flags.

### P9 — Legacy-vs-current module drift

- **Zephyr:** `zephyr/cmake/nros_generate_interfaces.cmake:91`
  hardcodes `find_program(... nros-codegen)` (a binary that no longer
  ships; only `nros` does). The `prj.conf` block tells users to set
  `CONFIG_NROS_C_API` + `CONFIG_NROS_RMW_ZENOH` — these symbols live
  only in the LEGACY `zephyr/Kconfig`, not in `integrations/zephyr/Kconfig`
  which the doc tells them to enable.
- **ESP-IDF:** dir basename is `esp-idf`; IDF resolves `REQUIRES nano-ros`
  by component name = basename = `esp-idf`, not `nano-ros`. Every
  consumer `idf.py set-target` fails at component resolution.

**Hit by:** integration-zephyr, integration-esp-idf.
**Fix:** rename `integrations/esp-idf/` → `integrations/nano-ros/`;
fold the legacy `zephyr/cmake/` shim into `integrations/zephyr/` (or
delete + update the doc); audit the `integrations/zephyr/Kconfig` for
the `_C_API` / `_RMW_<rmw>` symbols.

### P10 — Invented config knobs

`integration-platformio.md` documents `NANO_ROS_WIFI_SSID`,
`NANO_ROS_WIFI_PASSWORD`, `NANO_ROS_LOCATOR`, `NANO_ROS_DOMAIN_ID` as
`build_flags`. Zero hits across `packages/`, `examples/`, `integrations/`.

**Hit by:** integration-platformio (acute), esp32 (`SSID=…` env-var
override claim is build-time-only and misleading).
**Fix:** drop the section, or implement the macros (read in board crate
`build.rs` via `option_env!`).

### P11 — Wrong board-crate names / GitHub org

- `bare-metal.md`: `nros-board-stm32f4-nucleo` → real is `nros-board-stm32f4`.
- `threadx.md`: `nros-board-riscv64-qemu` → real is
  `nros-board-threadx-qemu-riscv64`.
- `integration-platformio` `library.json` + `library.properties`:
  `github.com/aeon/nano-ros` → should be `github.com/NEWSLabNTU/nano-ros`.
- `integration-esp-idf` `idf_component.yml`: same `aeon` → `NEWSLabNTU` fix.

**Fix:** mechanical replace + add a CI grep guard against `aeon/nano-ros`.

### P12 — Doc oversells what the template does

- `px4.md`: claims `INFO [nano-ros] bridge started` + data flowing in
  5 s. Template just registers + returns; no publisher loop, no
  `bridge started` log.
- `esp32.md`: claims `Published: 1` after `just esp32 talker`. Recipe
  boots QEMU but talker fails because no `zenohd` was started.
- `integration-platformio` `library.json`: doc says "PIO's lib resolver
  picks up the library spec, builds Rust staticlibs (~3 min first time)".
  Manifest has `srcFilter:["-<*>"]` — compiles nothing.

**Fix:** match prose to what the template actually does today. Either
ship a richer template or downgrade the prose.

### P13 — `just <platform>` recipe coverage gaps

- `just esp32 build` is a no-op stub printing "use 'just esp32
  build-examples'". Doc has the user run the stub.
- `just doctor tier=default` hangs on `_pinned-toolchain-files` (rustup
  network call); SIGTERM after 3 min. Per-platform `just <plat> doctor`
  is fast. Doc leads with the slow one.

**Hit by:** esp32, troubleshoot.
**Fix:** delete the no-op stubs OR make them call the real recipe; doc
leads with the scoped variant.

### P15 — `install-nros.sh` silently no-ops on existing PATH (stale-CLI wedge)

Every embedded tutorial opens with `curl … install-nros.sh | sh`. When an
older `nros` is already on `~/.nros/bin` (returning user, prior session),
the installer early-exits with "nros already on PATH; skipping" — it does
NOT bump the binary. Subsequent `nros setup …` / `just <plat> setup` fail
with `Error: invalid SDK index nros-sdk-index.toml … unknown field 'shallow'`
because the stale CLI (e.g. 0.2.0/0.3.1) can't parse the current index
schema (needs ≥0.3.7 per `scripts/install-nros.sh`). The user is wedged
with no instruction telling them to remove `~/.nros/bin/nros` first, pass
`NROS_VERSION=<pin>`, or use `--force`. Distinct from P4 (which fixed the
`zenohd` shim path but did NOT bump the nros binary itself; the installer's
own early-exit precedes the shim refresh block).

**Hit by:** every tutorial whose prereqs run the installer (installation,
first-node-{rust,c,cpp}, freertos, threadx, bare-metal, **integration-nuttx**
(Batch 3 re-audit, 2026-05-30 confirmed), integration-zephyr, esp32, esp-idf,
platformio, px4). Effectively cross-cutting.

**Fix candidates:** (a) `install-nros.sh` compares the installed CLI's
`--version` against `NROS_VERSION` and bumps when behind (default behaviour);
(b) add a `--force` / `NROS_INSTALL_FORCE=1` knob and instruct returning
users to re-run with it; (c) doc adds a "if you have an older `nros`, run
`rm -f ~/.nros/bin/nros` first" line to every prereq block. (a) is the
least-bad — silent stale-PATH wedge is a P1-class footgun.

### P14 — Misc per-page bugs

- `nuttx.md` line 124: `nros_talker` NSH command is fictional; real
  PROGNAMEs are `nuttx_c_talker` / `nuttx_cpp_talker` etc. Per-board
  defconfig + `kconfig-tweak` glue (~150 lines) hidden by the bare
  `cd $NUTTX_DIR && make` block.
- `esp32.md` line 145: `rustup target add xtensa-esp32s3-none-elf` —
  no such rustup target (Xtensa needs espup).
- `integration-zephyr.md` lines 266-270: `west patch` not available in
  this workspace's west (no extension registered).
- `px4.md` lines 52/72: `-DNANO_ROS_DIR=` is a cache var, but the
  template reads `$ENV{NANO_ROS_DIR}` — cache form silently doesn't
  propagate.
- `troubleshooting-first-10-min.md` symptom 1 fix is misleading:
  unresolved-import errors are path-dep breakage, not SDK fetch.
- `first-node-rust.md` lines 62-63: claims empty `[workspace]` table in
  example Cargo.toml; the example is actually a workspace member of
  root `Cargo.toml`.

## Batch 2 re-run — net-new findings (2026-05-30)

Strict-follow audit of `freertos.md` / `threadx.md` / `bare-metal.md` re-run
in fresh worktrees against current `main`. Per-tutorial reports +
`SUMMARY.md` preserved at `docs/roadmap/book-audit/batch-2/`. The original
P1–P14 reinforced (see SUMMARY's confirmation matrix); four new patterns
surfaced:

### N1 — Installed `nros` CLI ≤ index requirement is not enforced

Every batch-2 tutorial dies at the first `nros setup …` with
`unknown field 'shallow'`. Host's `~/.nros/bin/nros` is 0.3.1; the in-tree
`nros-sdk-index.toml` uses `[source.*] shallow = true` (Phase 197.2 +
Phase 207 follow-up) which the CLI rejects unless it's ≥ 0.3.2.
`scripts/install-nros.sh` pins `NROS_VERSION=0.3.7` but its first guard
`if command -v nros … exit 0` lets a stale install slip through.

**Hit by:** every batch-2 tutorial + every other tutorial that begins with
`nros setup`. Until N1 is fixed, no later prerequisite can be tested.

**Fix candidates (priority order):**
1. `install-nros.sh` compare `nros --version` against `NROS_VERSION` and
   re-install on mismatch. One-line change with global reach.
2. `nros setup` preflight that detects an older parser handed a newer
   index + emits a real upgrade hint instead of `unknown field …`.
3. Every tutorial leads with `sh scripts/install-nros.sh` (doc churn, the
   weakest of the three).

### N2 — `[workspace]` table missing from example `Cargo.toml`s

`cargo build` from `examples/<plat>/<lang>/<example>` inside any nested
checkout fails:

```
current package believes it's in a workspace when it's not;
workspace: /home/aeon/repos/nano-ros/Cargo.toml
```

Example `Cargo.toml`s have **no empty `[workspace]` table**; cargo walks
up + the outer nano-ros workspace adopts them. Hit on freertos/Rust,
bare-metal/Rust, `just freertos build-fixtures`. The README claim of
"each example is a standalone copy-out template" doesn't hold *as
written* — a real user vendoring an example into their own workspace
hits the same upward-walk.

**Fix:** every `examples/<plat>/<lang>/<example>/Cargo.toml` ships an
empty `[workspace]` table. ~80 example dirs; mechanical, one line per
file. (P14 already noted this for `first-node-rust.md` specifically; N2
generalises it to the example fleet.)

### N3 — Banner text in tutorials is fictional

`bare-metal.md`, `threadx.md`, `freertos.md` all show an expected
`nros <Platform> <Role> Talker` banner that doesn't exist in any
`src/main.rs`. The actual output is the runtime's structured `nros_info!`
lines: `Declaring publisher on /chatter (std_msgs/Int32)`, `Publisher
declared`, `Published: 0`, … (P7's "start at 0" is the off-by-one).

**Fix:** delete the fake banners; quote the real first ≤ 5 log lines
from each example's `src/main.rs`. Pairs with P7.

### N4 — `~/.nros/bin` not auto-PATH

Every agent had to manually `export PATH="$HOME/.nros/bin:$PATH"` after
install. The install script prints a hint but the tutorials don't
reproduce it. Options:
- `install-nros.sh` writes a `~/.profile.d/nros.sh` (or appends to
  `~/.bashrc`/`~/.zshrc` with consent); or
- every tutorial leads with the `export PATH` line.

Same forget-risk trade-off as N1.

## Revised Phase 208.B priority order (Batch 2 input)

The matrix below already lines up the mechanical edits. Batch 2's input
on the **order of work**:

1. **N1 first.** Until `nros setup` works on a stale host, nothing else
   in batches 2–6 reaches a build step. One-line `install-nros.sh`
   change; biggest reach for least churn.
2. **P2** (`config.toml` → `nros.toml` + schema) — every batch-2
   tutorial produces an un-parseable file as written; can run in
   parallel with N1.
3. **N2** (empty `[workspace]` in every example `Cargo.toml`) — unblocks
   nested-checkout audits + the README's copy-out claim.
4. **P7 + N3** (banners + off-by-one) — cosmetic but visible; quick win.

Then the rest of the 208.B plan below as written.

## Recommended Phase 208.B doc-edit plan

The matrix above implies two follow-up tracks:

**A. Root-cause fixes (lands in tree, not just docs).**
1. P1 fix: export `NROS_PLATFORM_CFFI_INCLUDE` from `.envrc` (or fix
   `zpico-sys/build.rs` + `threadx-common/threadx_sources.rs` to
   autoresolve). Same for `NROS_PLATFORM_THREADX_SRC`.
2. P3 fix: gate `px4-sitl-tests` workspace dep behind a feature OR
   include `px4-rs` in the native plan.
3. P4 fix: `install-nros.sh` / `nros setup --rmw zenoh` symlinks
   `zenohd` into `~/.nros/bin/`.
4. P9 ESP-IDF fix: rename `integrations/esp-idf/` →
   `integrations/nano-ros/` (or add a `package_manager_files: { name:
   nano-ros }` if IDF supports it).
5. P9 Zephyr fix: replace `find_program(nros-codegen)` in
   `zephyr/cmake/nros_generate_interfaces.cmake` with the canonical
   `nros` find used by `cmake/NanoRosGenerateInterfaces.cmake`. Audit
   `integrations/zephyr/Kconfig` for missing `_C_API`/`_RMW_<rmw>`
   bools.
6. P11 fix: `aeon/nano-ros` → `NEWSLabNTU/nano-ros` everywhere in
   `integrations/`, with a CI grep guard.
7. P13 fix: delete `just esp32 build` stub; have the recipe call
   `build-examples` directly.
8. ~~**P15 fix (P1-class, cross-cutting):**~~ **DONE.** `scripts/install-nros.sh`
   now compares the installed `nros --version` against `NROS_VERSION`:
   skip when equal (idempotent), keep when installed is *newer* (no
   downgrade surprise), **auto-bump when behind**, and force-re-install
   on `NROS_INSTALL_FORCE=1`. The silent stale-PATH wedge is gone — a
   returning user re-running the installer gets the pinned CLI without
   instructions, and the prereq blocks across every embedded tutorial
   now "just work" instead of failing the next `nros setup`. B.10 below
   becomes a no-op and can be deleted next pass.

**B. Doc-only fixes (lands after A so the prose matches working state).**
1. Every embedded tutorial: rewrite Configure section against the real
   `nros.toml` schema (P2).
2. Every embedded tutorial: add "start `zenohd -l tcp/127.0.0.1:<port>`"
   step before the QEMU boot (P6).
3. Every embedded tutorial: replace direct `qemu-system-*` invocations
   with `just <plat> talker` for the happy path (P8). Cite the recipe
   if a direct command is needed.
4. `s/Published: 1/Published: 0/` across all tutorials (P7).
5. Align CMake snippets in `first-node-{c,cpp}.md` and Pattern B in
   `installation.md` with the canonical example shape (P5).
6. Drop / fix invented macros in `integration-platformio.md` (P10);
   replace the `.ini` snippet with one that actually configures.
7. `nuttx.md`: replace the fake NSH command with real PROGNAMEs;
   document the defconfig + `kconfig-tweak` glue (P14).
8. `troubleshooting-first-10-min.md`: rewrite symptom 1 (path-dep
   breakage, not SDK fetch); lead `just doctor` advice with the
   per-platform scoped variant (P14).
9. `px4.md`: `-D` → env-var (P14); downgrade the "bridge started"
   prose to match the template (P12).
10. ~~**P15 doc mitigation**~~ — **superseded by A.8** (installer self-bumps).
    No doc change needed.

## Worktrees preserved

`/home/aeon/repos/nano-ros/.claude/worktrees/agent-<id>/` per agent.
Some agents nested (their worktree was created inside an earlier
agent's worktree because the harness ran them sequentially). To
reproduce a specific finding, `cd` into the worktree path printed at
the top of each per-tutorial report.
