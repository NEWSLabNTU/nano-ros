# Phase 163 — Book Starters Revision

**Goal.** Replace the fragmented book entry path with a coherent
Getting-Started → Embedded-Starters flow that delivers per-language
(Rust / C / C++) single-node starter pages on Linux, plus per-RTOS
starter pages in each RTOS's native layout. Every starter follows the
same 5-section template (Layout / Configure / Build / Run / GitHub
link) so users can pattern-match across languages and platforms.

**Status.** **CLOSED 2026-05-19.** All work items landed in one session.
restructure) landed 2026-05-19; 163.C (Embedded Starters rewrites) landed same day; 163.D + 163.E landed 2026-05-19. **All done.**.

**Priority.** P2 — user onboarding quality. Current book has a single
Rust starter (`First Native Rust Node`); C and C++ users have no
equivalent entry point even though the example tree carries
first-class C and C++ talkers under `examples/native/{c,cpp}/zenoh/`.

**Depends on.** Phase 131 (examples layout), Phase 144 (add_subdirectory
consumption), Phase 161 (nros-cpp Cargo.toml fix). All landed.

---

## Problems with the current book entry

1. **Single starter, single language.** `First Native Rust Node` is
   the only walkthrough; C and C++ users hit `installation.md` →
   `package-preparation.md` and have to assemble the canonical layout
   themselves.
2. **Integration shells leak into "User Guide" too early.** Zephyr
   `west`, ESP-IDF, PlatformIO, NuttX, PX4 pages all sit above the
   first node walkthrough — RTOS decision is forced before users
   know what nano-ros code looks like.
3. **Layout / config / build / deploy fragmented.** Cross-page hops
   between `installation.md`, `package-preparation.md`, `workflow.md`,
   `deployment.md`. A new user reads four pages to get one starter.
4. **`Platform Guides` vs `Integration:` overlap.** Each RTOS has two
   entry pages (e.g. `getting-started/zephyr.md` and
   `getting-started/integration-zephyr.md`). Drift is real (different
   recipes, different cwd assumptions).

---

## Architecture

Five-section template, identical across languages + RTOSes:

```
# First Node — <lang> on <plat>

## Project layout          tree of the standalone copy-out
## Configure               config.toml | env vars | CMake cache | Cargo features
## Build                   one copy-pastable command
## Run                     POSIX terminal output or QEMU invocation + verify
## GitHub source           copy-out link to examples/...
## Next                    pointer to sub/service/action, custom msgs, RTOS port
```

Reader path:

```
Introduction
  └─ Getting Started (Linux first)
        ├─ What is nano-ros
        ├─ Setup Compared to Standard ROS 2
        ├─ Install + first build (Linux)
        ├─ First Node — Rust
        ├─ First Node — C
        └─ First Node — C++
  └─ Embedded Starters (per RTOS, native layout)
        ├─ FreeRTOS (QEMU MPS2-AN385)
        ├─ Zephyr   (west module)
        ├─ NuttX    (apps/external)
        ├─ ThreadX  (Linux sim / RV64 QEMU)
        ├─ ESP32    (esp-hal / ESP-IDF)
        ├─ Bare-metal Cortex-M3 (QEMU)
        └─ PX4 Autopilot
  └─ User Guide (operational reference, post-starter)
  └─ Concepts | Porting | Design | Internals | Reference | Release
```

---

## Work items

### 163.A — Linux starters

- [x] **163.A.1** Draft `getting-started/first-node-rust.md` from
      the existing `native.md`. Refit to the 5-section template;
      add GitHub link to `examples/native/rust/zenoh/talker/`.
- [x] **163.A.2** New `getting-started/first-node-c.md`. GitHub
      link `examples/native/c/zenoh/talker/`. Show
      `add_subdirectory(<repo-root>)` CMake snippet, `nros_app_main`
      entry shape.
- [x] **163.A.3** New `getting-started/first-node-cpp.md`. GitHub
      link `examples/native/cpp/zenoh/talker/`. Show
      `NROS_TRY_RET` macro + `nros::create_node` /
      `nros::create_publisher` chain.

### 163.B — SUMMARY.md restructure

- [x] **163.B.1** Add the new top-level `# Getting Started (Linux
      first)` section in `SUMMARY.md` containing the three
      first-node pages plus the existing `installation.md`
      reflavored as "Install + first build (Linux)".
- [x] **163.B.2** Add the new top-level `# Embedded Starters`
      section pointing at the existing per-RTOS pages. (The per-RTOS
      page bodies are revised in 163.C; this step only moves them
      in the TOC.)
- [x] **163.B.3** Collapse `# Platform Guides` into the new
      Embedded Starters section. Drop the `Integration:` duplicate
      entries from `# User Guide` (they live in Embedded Starters
      now).

### 163.C — Embedded starters (per-RTOS native layout)

Each RTOS page rewritten to its native layout convention.

- [x] **163.C.1** **FreeRTOS** (QEMU MPS2-AN385).
      Layout: standard nano-ros example tree
      (`examples/qemu-arm-freertos/<lang>/zenoh/talker/`). Rust / C /
      C++ all shown.
- [x] **163.C.2** **Zephyr** (west module). Layout: `samples/`
      directory under a west-managed workspace; `prj.conf` +
      `west build -b <board>` flow. Reuse `integrations/zephyr/`
      shell. Rust / C / C++.
- [x] **163.C.3** **NuttX** (`apps/external/`). Layout: NuttX
      app shim, Kconfig entry. Reuse `integrations/nuttx/`. Rust /
      C / C++.
- [x] **163.C.4** **ThreadX** (Linux sim and RISC-V64 QEMU).
      Layout: standard nano-ros tree under
      `examples/threadx-linux/` and `examples/threadx-riscv64/`.
      Rust + C only (no nros-cpp port).
- [x] **163.C.5** **ESP32** (esp-hal bare-metal + ESP-IDF
      component). Two halves: esp-hal Rust layout (no IDF), and
      ESP-IDF component layout via `integrations/esp-idf/`. C / C++
      for IDF path, Rust for esp-hal path.
- [x] **163.C.6** **Bare-metal Cortex-M3** (QEMU MPS2-AN385). Rust
      only; nros-c / nros-cpp not supported on bare-metal (CLAUDE.md
      coverage matrix).
- [x] **163.C.7** **PX4 Autopilot** (external module). Layout:
      `EXTERNAL_MODULES_LOCATION` pattern. C++ only (uORB binding
      is C++-only).

### 163.D — User Guide consolidation

- [x] **163.D.1** Drop `getting-started/native.md` (superseded by
      `first-node-rust.md`).
- [x] **163.D.2** Drop `user-guide/package-preparation.md`
      (subsumed by per-starter "Project layout" sections). Add a
      stub redirect or fold useful content into
      `user-guide/workflow.md`.
- [x] **163.D.3** Refit `getting-started/installation.md` to be
      pure environment-setup (just / setup.sh / SDK tiers) without
      duplicating the first-node walkthrough.
- [x] **163.D.4** Strip duplicate Integration vs Platform Guide
      pages — keep the integration shell (`west`, ESP-IDF
      component, etc.) page; merge the contributor-facing platform
      page into Internals.

### 163.E — Cross-cutting polish

- [x] **163.E.1** Every starter page links to its GitHub example
      directory with a `[`copy-out`](https://github.com/.../tree/...)`
      anchor.
- [x] **163.E.2** Every starter page ends with a "Next" section
      listing 3 next steps (add a sub, add custom msgs, cross-
      compile for an RTOS).
- [x] **163.E.3** `book/src/SUMMARY.md` lints clean against the
      mdbook build.

---

## Files

### New

- `book/src/getting-started/first-node-rust.md`
- `book/src/getting-started/first-node-c.md`
- `book/src/getting-started/first-node-cpp.md`

### Renamed

- `book/src/getting-started/native.md` → deleted in 163.D.1.
- `book/src/user-guide/package-preparation.md` → deleted in 163.D.2.

### Modified

- `book/src/SUMMARY.md` — section restructure.
- Per-RTOS pages under `book/src/getting-started/{freertos,zephyr,nuttx,
  threadx,esp32,bare-metal,px4}.md` — refit to 5-section template.
- `book/src/getting-started/integration-*.md` — kept; cross-linked
  from the per-RTOS Embedded Starter page when an RTOS package
  manager (`west`, `idf.py`, PIO) is the canonical consumer.

---

## Acceptance criteria

- [ ] Three Linux starter pages exist (Rust / C / C++) following
      identical 5-section template.
- [ ] Each Linux starter links to a working GitHub example dir.
- [ ] Embedded Starters section reachable as a top-level group in
      the rendered book TOC; no duplicate "Integration vs Platform
      Guide" entries.
- [ ] Per-RTOS starter pages use that RTOS's native layout
      (Zephyr `samples/`, ESP-IDF `idf_component_yml`, NuttX
      `apps/external/`, PX4 `EXTERNAL_MODULES_LOCATION`).
- [ ] `mdbook build` is clean.
- [ ] Reader can go from `git clone` to a running Linux talker /
      listener in <10 minutes using only the Getting Started
      section.

---

## Notes

- Linux is the lead because it's the most-familiar dev environment
  and exposes the same API surface as embedded targets. Once the
  reader has a Linux node working, cross-compiling for an RTOS is
  a small delta (different `set(NANO_ROS_PLATFORM …)` + RTOS-side
  build invocation).
- Each per-RTOS page uses **that RTOS's preferred layout**, not the
  nano-ros canonical tree. Zephyr users expect `samples/` and
  `prj.conf`; ESP-IDF users expect `main/idf_component_yml`. The
  in-tree examples under `examples/qemu-arm-freertos/` etc. are
  authoritative for the nano-ros canonical tree, and the GitHub
  copy-out link remains the implementation reference, but the
  starter page leads with the RTOS-native shape.
- Implementation order: **163.A → 163.B → 163.C → 163.D → 163.E**.
  Land 163.A + 163.B first so the Linux starter path is usable
  before the per-RTOS rework lands.
