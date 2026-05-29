# Phase 197 — `just setup` recipes onto canonical `nros setup`

**Goal.** Make `nros setup` the single provisioning entrypoint and turn the
`just <module> setup` recipes into thin callers of it, removing the duplicate
provisioning logic + manifests that drift from the SDK index.

**Status.** In progress (2026-05-29). **197.1/197.2/197.3/197.5 DONE**; only
197.4 (the `tools/setup.sh` split into provisioning + host-env) remains (P3).

**Priority.** P2 — no product capability depends on it, but the duplication is a
live drift surface (a source must be registered in *two* places to be both
fetched and nros-provisioned), and the zephyr gap (below) means local
`just zephyr setup` diverges from CI.

**Depends on.** Phase 187 (`nros setup --tool`), Phase 191.6 (`[board.*]`/`[rmw.*]`
+ board-driven `nros setup <board>`), Phase 195.B (`nros setup --source`,
index = SSOT for source refs).

---

## Background — what `nros setup` already owns

`nros setup` is canonical for SDK/toolchain/source provisioning from
`nros-sdk-index.toml`:
- `nros setup <board> [--rmw <rmw>]` → resolves `board.packages ∪ rmw.packages`
  (tools + sources) and provisions them.
- `nros setup --tool <name>` → one prebuilt/source-built host tool.
- `nros setup --source <name>` → one `[source.*]` submodule (index-driven
  `dest`/`ref`/`submodule`; runs `git submodule update --init -- <path>`).

The `just` recipes are at three levels of adoption (review 2026-05-29):

| Recipe(s) | Provisioning path | Canonical? |
|---|---|---|
| `qemu`, `zenohd` | `nros setup --tool <x>` directly | ✅ thin caller |
| `freertos`, `threadx-{linux,riscv64}`, `nuttx`, `cyclonedds`, `xrce` | `tools/setup.sh --platform/--rmw` → routes index-owned `[source.*]` through `nros setup --source` (git fallback when `nros` unbuilt) | ✅ via shim |
| `esp32` | `scripts/esp32/install-espressif-qemu.sh` directly | ❌ bespoke |
| `px4` | inline `git submodule update … PX4-Autopilot` | ❌ inline |
| `zephyr` | `scripts/zephyr/setup.sh` — own flow, **does not provision sources** | ❌ gap |

~~`tools/setup.sh` reads **two** manifests~~ — **as of 197.2 it reads only the
SDK index** (`config/submodule-deps.toml` retired): platform/rmw →
`packages`+`build_sources` → `[source.*]`.

---

## Work Items

### 197.1 — [DONE] Close the zephyr local-vs-CI gap
`scripts/zephyr/setup.sh` patches `third-party/dds/cyclonedds` and the zenoh
examples build `packages/zpico/zpico-sys/zenoh-pico`, but the recipe **assumes
both submodules are already checked out** (`config/submodule-deps.toml` lists
zephyr `paths=[]`, "uses west"). On a fresh clone, local `just zephyr setup`
can't patch cyclonedds. Phase 196 taught the *CI workflow* to
`nros setup --source zenoh-pico --source cyclonedds-src --source px4-rs`; the
local recipe must do the same so **local == CI**.
- [x] **DONE.** The `just zephyr setup` recipe now provisions `zenoh-pico` +
      `cyclonedds-src` + `px4-rs` via `nros setup --source` at the top (before the
      cyclonedds patches + the west setup), resolving nros from
      `$NROS_CLI`/PATH/`~/.nros` via `nros_cli_bin`. A fresh-clone local
      `just zephyr setup` now provisions exactly what the CI workflow does →
      **local == CI**. Verified the provision command resolves + provisions all
      three sources. (The CI workflow keeps its explicit pre-step too — idempotent
      belt-and-suspenders; the dep-chain/core-libs lanes don't run `just zephyr
      setup`.)

**Files**: `just/zephyr.just`.

### 197.2 — [P2] Retire `config/submodule-deps.toml` (single manifest) — DONE
A source used to appear in `submodule-deps.toml` (to be fetched by
`tools/setup.sh`) *and* in `[source.*]` (to be nros-provisioned) — a drift
surface. Now the index is the single home.
- [x] Modeled the ~12 build-time/dev/reference submodules the index didn't cover
      as `[source.*]` (mbedtls, micro-cdr, micro-xrce-dds-client, threadx-netxduo,
      nuttx-kernel, nuttx-apps, px4-autopilot, tracing, + dev source-repos). They
      stay **out of `packages`** (so `nros setup <board>` is unchanged); the
      build sources a local `just <plat> setup` needs are listed in new
      `build_sources` / `dev_sources` fields on `[board.*]`/`[rmw.*]` and a
      `[reference.*]` grouping.
- [x] `tools/setup.sh` derives its source set from the index: platform → boards
      (matched by `[board.*].platform` or board-id) → `packages`+`build_sources`;
      rmw → `packages`+`build_sources`; `--with-dev` adds `dev_sources`;
      `--with-reference` adds `[reference.*].sources`. Non-source names (host
      tools) are filtered; each source name → submodule path via `[source.*]`.
      Verified `--dry-run` reproduces the old per-platform/rmw source sets.
- [x] Deleted `config/submodule-deps.toml`; repointed `cmake/bootstrap.cmake`'s
      source-tree existence check to `nros-sdk-index.toml`; extended
      `scripts/sdk/verify-index.py` to validate `build_sources`/`dev_sources`/
      `[reference.*].sources` resolve to `[source.*]` (gate green).

**Files**: `tools/setup.sh`, `nros-sdk-index.toml`, `cmake/bootstrap.cmake`,
`scripts/sdk/verify-index.py`, `config/submodule-deps.toml` (deleted).

### 197.3 — [P3] Fold `esp32` + `px4` provisioning into the index — DONE
- [x] Espressif qemu fork → `[tool.esp32-qemu]` (source-built; `[tool.*.source]`
      configure/install mirroring `[tool.qemu]`, no dist). `just esp32 setup` →
      `nros setup --tool esp32-qemu` (behind the existing esp32c3-machine probe).
      Deleted the bespoke `scripts/esp32/install-espressif-qemu.sh` (its logic is
      the index recipe now); the redundant `[source.esp32-qemu-src]` 197.2
      dev-source was dropped (the tool clones the fork itself).
- [x] PX4-Autopilot → `[source.px4-autopilot]` + `just px4 setup` →
      `nros setup --source px4-rs --source px4-autopilot` (dropped the inline
      `git submodule update`). PX4's own ~50 nested sub-submodules stay a
      `git -C … submodule update --recursive` (PX4's concern, not nano-ros source
      provisioning), as does the `pip install` host-env step (197.4 scope).

### 197.5 — [DONE] 197.2 index schema needs nros-v0.3.1 (was P0 BLOCKER)
197.2 added `build_sources` / `dev_sources` to `[board.*]`/`[rmw.*]`. Those are
consumed only by `tools/setup.sh` (awk) + `verify-index.py` (python) — **not by
the nros CLI** — but the released **nros 0.3.0** loads the whole index with
`#[serde(deny_unknown_fields)]` and **rejects** them:
`invalid SDK index … TOML parse error at line N: build_sources`. This breaks
**every** CI lane that calls `nros setup` on the released binary (dep-chain,
core-libs `--source px4-rs`, all zephyr jobs). Decision (2026-05-29): cut a new
release. **RESOLVED 2026-05-29.**
- [x] **nros-cli** (`NEWSLabNTU/nros-cli`): added `build_sources`/`dev_sources` to
      `BoardEntry`/`RmwEntry` + a `[reference.*]` (`ReferenceEntry`) map to
      `SdkIndex` (parsed + ignored by the CLI). Cut **nros-v0.3.1** (commit
      `1071b54`, tag pushed → `release-binary.yml` published the 3 host assets).
      132 lib tests pass; verified the binary parses the 197.2 index + board
      resolution unchanged.
- [x] **superproject bump**: `[tool.nros]` → `0.3.1` (version/upstream + dist
      urls + the 3 new sha256s); `NROS_VERSION=0.3.0` → `0.3.1` at all 6 pin
      sites (ci, dep-chain, zephyr-dual-line ×3, nros-acceptance). install.sh
      0.3.1 verified e2e (installs + parses the 197.2 index).

**Files**: `nros-sdk-index.toml`, `just/esp32.just`, `just/px4.just`,
`scripts/esp32/install-espressif-qemu.sh`.

### 197.4 — [P3] `just <module> setup` = thin `nros setup <board>` + host-env step
The endgame: a module recipe is `nros setup <board>` (tools + sources from the
index) **plus** a separate host-env step for what's outside nros scope (apt
packages, rustup toolchains/targets, platform post-steps like NuttX external-app
staging, zephyr west-update). Retire `tools/setup.sh`'s platform branching.

- [x] **Approach A — `nros setup <board>` is now the complete provisioner.**
      Folded the 197.2 `build_sources` into the relevant `[board.*]`/`[rmw.*]`
      `packages` (e.g. qemu-arm-nuttx now lists nuttx-kernel/apps; rmw.zenoh lists
      zenoh-pico+mbedtls). A bare-machine `nros setup <board> → build` now works
      for every board (`build.rs` gates on these). **Constraint found:** the
      released `nros` parses `[rmw.*]`/`[board.*]` with a strict schema (only
      `packages` + board descriptors) and rejects unknown fields/sections — so the
      197.2 `dev_sources` field + `[reference.*]` section (which broke
      `nros setup <board>`) were removed; opt-in dev/interop sources stay plain
      submodules until an nros-cli schema change (see workflow review below).
- [x] **Builds consume nros-store tools (the prerequisite).** `setup.{bash,fish}`
      glob `~/.nros/sdk/<tool>/<ver>/bin` onto PATH, so cargo cross-builds use the
      pinned index toolchains (arm-none-eabi-gcc, qemu, …) rather than apt's.
      Verified: `nros setup --tool arm-none-eabi-gcc` → store 13.2 resolves ahead
      of apt 10.3; `just nuttx build-fixtures` builds C/C++/Rust green on 13.2.
- [x] **Recipe rewrite.** Added `tools/host-env.sh <platform>` (rustup install +
      cross target + apt hint — the host-local bits outside nros scope). Rewired
      `just <module> setup`: freertos → `nros setup qemu-arm-freertos`, nuttx →
      `qemu-arm-nuttx` (+ external-app staging), threadx-linux → `threadx-linux`,
      threadx-riscv64 → `qemu-riscv64-threadx`, cyclonedds →
      `nros setup --source cyclonedds-src` — each `+ tools/host-env.sh`. The
      platform→board map is now explicit per recipe (threadx's two boards become
      two recipes; cyclonedds is rmw-only via `--source`). `rmw_zenoh` keeps its
      bespoke ROS 2 overlay build (an interop dev fixture, not board provisioning).
      `tools/setup.sh` is retired from the recipes (still used by
      `cmake/bootstrap.cmake`'s auto-bootstrap). Custom-board provisioning (board
      crate self-describes its source deps) remains the nros-cli follow-up below.

**Files**: `nros-sdk-index.toml`, `tools/setup.sh`, `scripts/sdk/verify-index.py`
(done); `just/<module>.just` recipes (pending).

#### Workflow review — custom boards + "nros prepares the config a board crate needs"
*(Captured 2026-05-29 — informs the recipe rewrite + a likely nros-cli follow-up.)*

Today the SDK index `[board.*]` is the maintainer-owned SSOT: `nros setup <board>`
looks the board up there. A **user creating their own board crate** has no entry
there, so `nros setup <their-board>` can't know its source deps. The idea: a board
crate **declares its own source deps** (in a board manifest / `[package.metadata]`
/ a `nros-board.toml`), and `nros` reads that + provisions them + prepares the
build config (the `.cargo/config.toml` path-deps / cmake cache the crate needs).
This shifts the board→sources SSOT from the central index to the board crate,
which is what lets out-of-tree boards work. Needs nros-cli schema work (the strict
index parser is the blocker above); the central index becomes the registry for
*nano-ros's own* boards, user boards self-describe. To be scoped as a follow-up.

---

## Acceptance
- Fresh-clone local `just zephyr setup` provisions its sources (197.1).
- A source is declared in exactly one place — the SDK index (197.2).
- No `just` setup recipe inlines `git submodule update` / bespoke downloads for
  index-eligible packages (197.3/197.4).

## Notes
- `nros setup` scope is SDK/toolchain/source provisioning from the index — NOT
  apt packages or rustup. Host-env setup stays a separate concern (197.4); don't
  overload `nros setup` with it.
- Keep the `tools/setup.sh` git-fallback semantics (provision when `nros`
  unbuilt) wherever a recipe runs before the CLI is built — the codegen-submodule
  bootstrap chicken/egg still applies (see Phase 196 ci-conventions).
