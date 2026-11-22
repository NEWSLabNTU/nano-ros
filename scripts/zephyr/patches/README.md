# Zephyr per-line patch sets (Phase 199.3)

`just setup zephyr` applies a **version-dispatched** patch set:

```
scripts/zephyr/patches/<NROS_ZEPHYR_VERSION>.sh   # e.g. 3.7.sh, 4.4.sh
```

The recipe runs `bash scripts/zephyr/patches/${NROS_ZEPHYR_VERSION}.sh
"$WORKSPACE"` — there is **no inline `if version = …` branching** in
`just/zephyr.just` anymore.

## Adding a new Zephyr line

1. Confirm a `zephyr-lang-rust` revision builds against the target Zephyr
   (the support floor is **3.7** — see
   `docs/roadmap/phase-199-zephyr-version-support-policy.md`).
2. Add `west-<ver>.yml` pinning the `(zephyr × zephyr-lang-rust)` pair and the
   `NROS_ZEPHYR_VERSION == "<ver>"` branch in `just/zephyr.just`'s manifest/
   workspace selectors.
3. Drop `scripts/zephyr/patches/<ver>.sh` here — re-anchor only the
   native_sim / NSOS / CycloneDDS patches still un-upstreamed for that line.
   **No edit to the `setup` recipe.**

## Contract for a `<version>.sh`

- One positional arg: the Zephyr workspace dir.
- `cd`s to the repo root (works regardless of caller cwd).
- Every patch must be **idempotent** (`just setup zephyr` re-runs them).
- Every patch script has a **check mode**: it calls `nros_patch_would_apply
  <tag> <what>` (`scripts/lib/zephyr-patchset.sh`) immediately before each
  write, after the guard that found its edit absent. Under `NROS_PATCH_CHECK=1`
  that call exits 1 naming the edit, so the script writes nothing.
  `check-zephyr-pin-drift.py` runs the line's set that way against a
  workspace, which asks the TREE whether this checkout's edits are present
  (issue 2831), and `check-zephyr-patch-check-mode` refuses a set that names a
  script without the call. A set's non-edit steps are skipped under
  `nros_patch_checking`.
- The set's LAST step, outside check mode, is `nros_zephyr_patchset_record`,
  which notes in `<ws>/.west/nros-patchset` that this set ran. Nothing reads it
  as a verdict any more. `sync-workspace-manifest.sh` re-runs the set after
  every `west update`, because the update resets the set's edits.
- `zephyr/` is a **hard** requirement: a patch whose Zephyr target file is
  missing exits non-zero. `modules/lang/rust` is **optional** — a C/C++-only
  workspace has no zephyr-lang-rust (book: `zephyr-west-app.md`), and
  `nros setup board <b> --zephyr-workspace <ws>` runs this set against it. A
  patch that edits only that module prints a `[skip] … no zephyr-lang-rust
  module` line and exits 0; a script that patches both (the Cortex-A9 one)
  still applies its Zephyr half. Never fail the set over an absent optional
  module, and never skip without printing why.
- Most patches edit Zephyr *internals* (native_sim/NSOS — not stable APIs); these
  are the churn Phase 199 tracks for upstreaming. As fixes land upstream, drop
  the corresponding line so the carried set shrinks per release.
