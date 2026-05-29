# Phase 197 — `just setup` recipes onto canonical `nros setup`

**Goal.** Make `nros setup` the single provisioning entrypoint and turn the
`just <module> setup` recipes into thin callers of it, removing the duplicate
provisioning logic + manifests that drift from the SDK index.

**Status.** Proposed (2026-05-29). Findings captured from a review during Phase
196 CI bring-up; no code changes yet (maintainer chose "document, decide later").

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

### 197.1 — [P1] Close the zephyr local-vs-CI gap
`scripts/zephyr/setup.sh` patches `third-party/dds/cyclonedds` and the zenoh
examples build `packages/zpico/zpico-sys/zenoh-pico`, but the recipe **assumes
both submodules are already checked out** (`config/submodule-deps.toml` lists
zephyr `paths=[]`, "uses west"). On a fresh clone, local `just zephyr setup`
can't patch cyclonedds. Phase 196 taught the *CI workflow* to
`nros setup --source zenoh-pico --source cyclonedds-src --source px4-rs`; the
local recipe must do the same so **local == CI**.
- [ ] `scripts/zephyr/setup.sh` (or the `just zephyr setup` recipe) provisions
      `zenoh-pico` + `cyclonedds-src` (+ `px4-rs` for the root-workspace cargo
      load) via `nros setup --source` before patching, mirroring the CI workflow.

**Files**: `scripts/zephyr/setup.sh`, `just/zephyr.just`.

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

**Files**: `nros-sdk-index.toml`, `just/esp32.just`, `just/px4.just`,
`scripts/esp32/install-espressif-qemu.sh`.

### 197.4 — [P3] `just <module> setup` = thin `nros setup <board>` + host-env step
The endgame: a module recipe is `nros setup <board>` (tools + sources from the
index) **plus** a separate host-env step for what's outside nros scope (apt
packages, rustup toolchains/targets, platform post-steps like NuttX external-app
staging, zephyr west-update). Retire `tools/setup.sh`'s platform branching.
- [ ] Split `tools/setup.sh` into: (a) provisioning → delegate entirely to
      `nros setup <board>`; (b) a host-env helper (`apt`/`rustup`/post-steps).
- [ ] Each `just <module> setup` = `nros setup <board>` + the host-env helper.

**Files**: `tools/setup.sh`, every `just/<module>.just` setup recipe.

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
