---
id: 729
title: "Bundle boards are invisible to `nros setup board` and `nros ws board-facts` — both still resolve the retired `packages/boards/nros-board-<name>` crate layout"
status: resolved
type: bug
area: cli
related: []
---

# 0729 — bundle boards invisible to `setup board` / `ws board-facts`

Phase-337 W9.a folded the Zephyr board crates into conf bundles:
`nros-board-fvp-aemv8r-smp` now lives at
`packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/` (board.cmake +
prj.conf + overlays + west-downstream.yml). The board KEY is unchanged and the
bundle-aware resolver exists — `nros board info fvp-aemv8r-smp` finds it and
prints the full provisioning contract.

Two CLI verbs never got that resolver, and both still build the pre-337 path
`packages/boards/nros-board-<name>` directly:

1. **`nros setup board <name> --zephyr-workspace <dir>`** —
   `nros-cli-core/src/cmd/setup.rs` (`run_board`, the `crate_dir` join around
   line 459) fails hard:

   ```
   Error: nros setup board: no board crate at
   `<root>/packages/boards/nros-board-fvp-aemv8r-smp` (check the board name;
   `nros board list` enumerates them)
   ```

   This is the RFC-0014 §"Downstream Zephyr consumer provisioning" entry
   point — the ONE command the book tells a downstream consumer to run — so
   every bundle board (i.e. every in-tree Zephyr board) is unprovisionable
   through the sanctioned path. The four steps still work by hand
   (`nros setup --source <rmw-src>`, `scripts/zephyr/patches/<line>.sh`,
   `rustup target add`, lang-rust presence check), which is what the
   autoware-safety-island bootstrap now inlines as a workaround.

2. **`nros ws board-facts <ws> --board <name>`** (the phase-351 W5 cmake lane,
   `nros_resolve_board_facts()`) — degrades rather than fails: configure prints

   ```
   nano-ros: board facts NOT delivered from <app> — Error: no board descriptor
   claims `fvp-aemv8r-smp` (deploy `fvp`). Descriptors are matched by their
   `names` and by their directory (`packages/boards/nros-board-<name>`).
   ```

   so a Zephyr consumer on a bundle board silently loses the board-facts /
   site-config rung (tier core-pin, RT/exec model knobs).

## Repro

From a downstream Zephyr workspace (autoware-safety-island shape, nano-ros
checkout at `modules/nros`):

```sh
cd modules/nros
./packages/cli/target/release/nros board info fvp-aemv8r-smp     # works
./packages/cli/target/release/nros setup board fvp-aemv8r-smp \
    --zephyr-workspace "$PWD/../.."                              # fails as above
```

Observed at `eace28852`; both sites unchanged at `2a891e5aa` (2026-08-20).

## Fix shape

Fix the CLASS, not one verb: both sites should resolve through the same
bundle-aware board resolver `nros board info` uses (name → crate-or-bundle
dir), then read the provisioning contract from `board.cmake` /
`[package.metadata.nros.board]` as today. Sweep for further
`format!("nros-board-{...}")`-style path builds in `packages/cli` while
there — directory-derived board matching is the pattern that broke, twice in
one consumer bring-up.

## Downstream workaround (until fixed)

autoware-safety-island `scripts/bootstrap-asi.sh` inlines the four
`run_board` steps; its build exports `NROS_REPO_DIR` for the board-facts lane
(which then still degrades per (2), non-fatally).

## Resolution (2026-08-20)

Fixed the class, in three pieces:

1. **One contract loader** — `board_metadata::load_provisioning_contract(dir)`
   reads `Cargo.toml` `[package.metadata.nros.board]` when present, else the
   `board.cmake` face via the new `BoardMetadata::from_board_cmake` (required
   mirrored keys are errors naming the key; `REQUIRES_RUST`/`RUST_TARGETS`/
   `GATED_PKGS` get the bool/semicolon-list treatment cmake cannot spell in
   TOML).
2. **`nros setup board`** resolves through `locate_board_crate` (the same
   bundle-aware resolver `nros board info` uses) + that loader. The issue's
   repro now provisions: `nros setup board fvp-aemv8r-smp --zephyr-workspace …
   --dry-run` prints the full 4-step contract.
3. **`BoardCatalog::load` attaches bundle ALIASES** — each
   `nros-board-<family>/boards/<name>/board.cmake` adds `<name>` and its
   `NROS_BOARD_ZEPHYR_ID` to the `names` of the single descriptor whose
   platform matches the family, only when nothing already claims the name. So
   `resolve_deploy("fvp-aemv8r-smp")` — and with it `board-facts` and the
   site-config gate, which share that one rule per issue 0606 — lands on the
   zephyr descriptor instead of Unknown. Verified:
   `nros ws board-facts <ws> --deploy fvp` emits `NROS_BOARD=fvp-aemv8r-smp` +
   `NROS_BOARD_TOML=packages/boards/zephyr/nros-board.toml`.

Sweep (`format!("nros-board-…")` path builds in `packages/cli`): the two other
sites are the resolver itself and `new-platform`'s crate scaffold — both
correct. Tests: `bundle_board_cmake_face_yields_the_provisioning_contract`,
`board_cmake_face_missing_required_key_errors`,
`a_conf_bundle_board_resolves_to_its_family_descriptor`,
`bundle_aliases_never_shadow_an_authored_name`.

The autoware-safety-island `bootstrap-asi.sh` inline workaround can be dropped.
