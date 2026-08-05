---
id: 437
title: "`just px4 build-sitl-cpp` hardcodes `--release` and `target/release/`, so `just check-fast` is RED on main"
status: open
type: bug
area: build
related: [issue-0362, phase-338]
---

## Symptom

`just check-fast` fails on `main` — not in a feature branch, on `main`:

```
[FAIL] hardcoded cargo profile flag: just/px4.just:168:    NROS_PX4_BRIDGE_GEN="$GEN" cargo build --release \
[FAIL] hardcoded cargo profile flag: just/px4.just:172:    cargo build -p nros-cpp --no-default-features --features std,rmw-zenoh-cffi --release
[FAIL] hardcoded cargo profile flag: just/px4.just:173:    cargo build -p nros-rmw-zenoh-staticlib --features platform-posix,std --release
[FAIL] hardcoded profile directory: just/px4.just:181:    NROS_PX4_BRIDGE_FFI_ARCHIVE="$EXT_DIR/ffi/target/release/libnros_px4_bridge_ffi.a" \
error: recipe `check-build-profile-literals` failed
```

Reproduced against a PRISTINE `origin/main` worktree (`git worktree add /tmp/wt
origin/main`, then `bash scripts/check-build-profile-literals.sh`) — the gate
fails there on its own, so this is not an interaction with any in-flight branch.

Introduced by `e2f850efa` ("feat(#0362 pass 2): PX4 uORB->RMW bridge module —
builds + links; runtime blocked on #0436"), which added the `build-sitl-cpp`
recipe.

## Why the gate objects

Three `cargo build --release` calls plus one `target/release/` path spelling.
The repo's rule is that a build site asks for the ACTIVE profile rather than
naming one, because the profile is chosen upstream of the recipe and a literal
silently diverges from it — the `nros-relwithdebinfo` / `nros-minsizerel`
profiles exist precisely so a lane can pick, and a site that says `--release`
opts itself out without saying so.

The fourth hit is the one that actually bites: line 181 hands PX4's build the
FFI archive by PATH. If the profile ever moves, the `cargo build` on line 168
writes to one directory and line 181 reads from another, so PX4 links a STALE
archive or none — and the failure surfaces inside PX4's make, far from here.

## Fix shape

The propagation helpers already exist; the recipe just has to use them
(`scripts/build/cargo.sh`):

```sh
source scripts/build/cargo.sh
cargo build $(nros_cargo_profile_arg_string) --manifest-path "$EXT_DIR/ffi/Cargo.toml"
…/"$(nros_cargo_target_profile_dir)"/libnros_px4_bridge_ffi.a
```

Both the build and the archive path then derive from ONE answer, which is the
property line 181 currently lacks.

If the PX4 lane is genuinely meant to be release-only — plausible, since SITL
performance is the point — that is a legitimate answer, but it has to be stated
rather than implied. The gate takes a marker:

```
# profile-literal-ok: <one of host tool | vendored | benchmark | symbol fixture | unprofiled | dir vocabulary>
```

with `just/ros-editions.just:62` and `scripts/build/link-determinism-fixture.sh:41`
as precedent for the `unprofiled` reason. Even then, line 168 and line 181 must
agree by construction, not by both happening to say `release`.

## Impact

`just check-fast` is the cheapest gate in the repo and the one every other task
runs first. While this is red on `main`, every unrelated change looks like it
broke something, and the honest workflow ("green locally before pushing") is
impossible to follow for anyone who has not already learned to ignore this
particular failure — which is exactly how a gate stops being believed.

## Related

- issue 0362 — the PX4 uORB→RMW bridge this recipe builds.
- `scripts/check-build-profile-literals.sh` — the gate, including the marker
  syntax and the accepted reasons.
