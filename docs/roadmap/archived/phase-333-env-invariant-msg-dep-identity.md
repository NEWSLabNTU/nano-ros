# Phase 333 — Env-invariant message-dependency identity (path deps + 0.0.0)

> **W1–W5 landed 2026-08-02** (`93aa02016` + the docs commit that follows it).
> Two premises changed under contact and are worth reading before the next pass:
>
> - **D2 needed a code change after all.** The RFC says "no codegen change is
>   required" because `cargo_nros.toml.jinja` already emits `0.0.0` — but a
>   SECOND emitter, `rosidl-bindgen`'s `generate_cargo_toml`, still wrote the
>   ament version, and it produced the `4.9.1` crates W3 found. Fixed in
>   `c673faa40`.
> - **W3 did not need a ROS host.** Every leaf with a committed `generated/`
>   tree uses bundled interface packages, so codegen produced `0.0.0` on a
>   ROS-less Arch host. Leaves without a `package.xml` cannot be reached by
>   `nros sync` at all; their manifests were edited mechanically and verified
>   byte-identical to the generator's own output.
>
> Left open deliberately: RFC-0067 Q1 (`nros-core`/`nros-serdes` still reached
> by registry name + a cwd-dependent config patch — from the repo root a
> config-patched leaf still fails `no matching package named nros-core`), and
> the two unsynced leaves whose tracked locks are now stale-by-construction
> (`tests/simple-workspace`, `packages/cli/testing_workspaces/complex_workspace`).

**Implements:** RFC-0067. **Closes:** issue 0378 (and retires its interim
`check-msg-dep-redirect`). **Touches:** RFC-0026 §Cargo.lock (adds the
testing/bench committed-lock class), RFC-0048 W9 (retires the message-crate
`[patch.crates-io]`).

> **Prototype-validated.** RFC-0067 §Evidence converted one leaf (`int32-sink`)
> by hand and proved the shape works end-to-end (build, unification, env-invariant
> lock, no crates.io from root). This phase generalises that across every leaf +
> adds the enforcing gate + regenerates the stale committed crates. Do NOT
> re-derive the design — read RFC-0067 first.

## Goal

Every reference to a generated ROS message crate resolves to a `generated/`
**path** (never a crates.io registry name), and every generated crate is
`version = "0.0.0"`, so committed leaf locks are byte-identical across ROS
distros and no message name is ever resolved against crates.io.

## Inventory (do this first)

- [ ] Enumerate leaves declaring a message dep by registry name:
      `bash scripts/check-msg-dep-redirect.sh` lists them today (its
      `MSG_CRATES` set is the name list), or
      `git grep -nE '^(std_msgs|builtin_interfaces|example_interfaces|geometry_msgs|sensor_msgs|action_msgs|nav_msgs|diagnostic_msgs|unique_identifier_msgs|test_msgs|rosgraph_msgs|trajectory_msgs|shape_msgs|stereo_msgs|visualization_msgs|lifecycle_msgs) = \{ version'`.
      Record which have their patch in **manifest** `[patch.crates-io]`
      (e.g. `nros-tests/bins/int32-sink`, committed → already safe from the
      cwd hole) vs **config** `.cargo/config.toml` (e.g. `nros-bench/stress-zenoh`,
      `nros sync`-managed). Both convert the same way; note which commit their
      `generated/` tree (only those can build offline).

## W1 — Leaf message deps: registry → path

**Files:** each leaf's `Cargo.toml` (the `[dependencies]` message lines and the
message entries under `[patch.crates-io]`).

- [ ] For each message dep, rewrite
      `<msg> = { version = "*", default-features = false }`
      → `<msg> = { path = "generated/<msg>", default-features = false }`.
- [ ] Delete the now-redundant **message** entries from `[patch.crates-io]`
      (manifest or config). KEEP `nros-core` / `nros-serdes` entries (out of scope
      — RFC-0067 Open questions).
- [ ] Leaves whose message dep is only TRANSITIVE (via another message crate)
      need no leaf line — the generated crate already path-links its deps
      (`builtin_interfaces = { path = "../builtin_interfaces" }`). Do not add a
      leaf line for a crate the leaf does not directly use.
- [ ] Verify on a leaf that commits `generated/`:
      `NROS_CARGO_FLAGS= cargo build` succeeds and
      `NROS_CARGO_FLAGS= cargo tree | grep -E '<msg>' | sort -u` shows exactly one
      copy of each message crate (unification). Repeat the RFC-0067 from-root
      check: `cargo metadata --manifest-path <leaf> --offline` → the message
      package id is `path+file://…`, never `registry+…`.

## W2 — Enforcing gate: message deps must be `path`

**Files:** `scripts/check-msg-dep-is-path.sh` (new, replaces
`scripts/check-msg-dep-redirect.sh`), `justfile` (`check-fast` list ~line 358).

- [ ] The gate walks every in-tree leaf `Cargo.toml` and FAILS if any message
      crate (the `MSG_CRATES` set) is declared as a registry dep
      (`version = …`) rather than a `path` dep. This is stronger than the interim
      redirect check (which only required *some* redirect up the config chain) and
      needs no config-chain walk — a path dep is safe from every cwd.
- [ ] Swap the recipe name in `check-fast`; delete `check-msg-dep-redirect.sh`.
- [ ] Message: name the offending leaf + crate + "declare it as
      `{ path = \"generated/<crate>\" }` (RFC-0067 D1)".

## W3 — Regenerate committed generated crates to `0.0.0` (NEEDS A ROS 2 HOST)

**Files:** committed `packages/**/generated/**/Cargo.toml` currently pinning an
ament version (e.g. `4.9.1`), and their leaf `Cargo.lock`s.

- [ ] Confirm current codegen emits `0.0.0`: it does — `cargo_nros.toml.jinja`
      hardcodes `version = "0.0.0"` + `ament_version = "{{ package_version }}"`.
      The committed `4.9.1` crates predate it.
- [ ] Regenerate each committed `generated/` tree (`nros sync` /
      `nros generate-rust`) on a ROS 2 ament host → the crates become `0.0.0`.
- [ ] Re-resolve + commit each affected leaf `Cargo.lock` (via
      `just lock-update`, never a bare `cargo generate-lockfile` — CLAUDE.md
      "Lockfiles"). The lock now pins `<msg> 0.0.0` with no registry source.

## W4 — Cargo.lock policy for testing/bench leaves (RFC-0026 refinement)

**Files:** `docs/design/0026-example-directory-layout.md` §Cargo.lock policy.

- [ ] Document the third class: an in-tree `packages/testing/{nros-bench,
      nros-tests/bins}/*` leaf that commits its `generated/` tree has an
      env-invariant message identity (D1+D2) and therefore commits a REPRODUCIBLE
      lock under `--locked`. A leaf that does NOT commit `generated/` keeps a path
      dep to an absent dir → fails closed → no committed lock. Decide + record
      which testing/bench leaves commit `generated/` (RFC-0067 Open question 2).

## W5 — Close the loop

- [ ] Update issue 0378 → resolved, move to `archived/`, `resolved_in` the W1/W2
      commit. Note the P1 exposure is closed structurally (path deps) and the P2
      `--locked` drift by D2.
- [ ] Cross-link RFC-0067 from RFC-0026 / RFC-0048 (the message-crate patch is
      retired) / RFC-0023.

## Acceptance

- `just check msg-dep-is-path` green; `git grep '<msg> = { version'` finds zero
  message registry deps in leaf manifests.
- From the repo root, `cargo metadata --manifest-path <any-leaf> --offline`
  shows every message package id as `path+file://…`, never `registry+…`
  (spot-check 3 leaves incl. one manifest-patched + one config-patched).
- Every committed leaf `Cargo.lock` pins its message crates at `0.0.0` with no
  registry source; `cargo build --locked` from a leaf passes on a host of a
  DIFFERENT ROS distro than the one that generated the lock (the drift that
  motivated this).
- `tier 1` (`just ci`) green.

## Risk / notes

- **ROS-host dependency (W3 only).** W1/W2 close the crates.io exposure with no
  ROS env; W3 (regen → `0.0.0`) needs one. Sequence so a partial landing (W1/W2
  without W3) still leaves the tree building — a path dep to a committed `4.9.1`
  crate builds fine; the lock just isn't yet distro-invariant.
- **`generated/`-absent leaves** (e.g. `stress-zenoh`) become build-blocked
  until `nros sync` after W1 — that IS the intended fail-closed, and matches how
  `examples/**` already behave, but confirm no tier-1 lane builds such a leaf
  without a prior sync.
- Do the leaf edits per-path (`git add <leaf>/Cargo.toml`), never `git add -A`
  (CLAUDE.md) — the regen in W3 also rewrites generated files.
