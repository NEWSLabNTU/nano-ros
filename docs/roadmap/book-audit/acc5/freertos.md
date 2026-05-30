BLOCKERS
1. `just freertos zenohd` (doc Run step 1) hardcodes `build/zenohd/zenohd`, which `nros setup qemu-arm-freertos --rmw zenoh` does not populate. Recipe exits 127 on a fresh post-`nros setup` machine. Doc presents this as the primary "Step 1" of Run and only offers the working inline `zenohd --listen tcp/...` form as a commented-out alternative.

FRICTION
- `just freertos talker` / `_run-qemu` hardcodes `build/qemu/bin/qemu-system-arm` (also not populated by `nros setup`), but falls back to system `qemu-system-arm` via `path_exists`. Works only if system QEMU is already installed — works on this host, would fail on a clean machine.
- Worktree artifact (not a doc bug): all `cargo build` paths fail with the "current package believes it's in a workspace when it's not" error because cargo walks up from the worktree's `examples/…` into the home repo's root `Cargo.toml`. On a fresh canonical clone this works (workspace `exclude` covers the examples). Reported as non-BLOCKER.

CLARITY
- Run flow CLEAR shape, blocked by the recipe path issue.

MISSING STEPS
- No fallback hint when `just freertos zenohd` fails (the inline `zenohd --listen …` form is buried as a commented-out line).

NITs
- Project-layout tree shows `.cargo/config.toml` + `generated/` for Rust talker but omits the on-disk `CMakeLists.txt`.
- Shows `package.xml` for C talker which isn't present on disk.

WORKS
- `nros 0.3.7 setup qemu-arm-freertos --rmw zenoh` (idempotent, exit 0).
- `nros.toml` doc block byte-accurate.
- GitHub source links resolve.

Acceptance bar (0 BLOCKERS): **NOT MET** — `just freertos zenohd` recipe still needs to be fixed (or the doc needs to lead with the inline `zenohd --listen ...` form).

LAST COMMAND: just freertos zenohd
LAST EXIT CODE: 127
