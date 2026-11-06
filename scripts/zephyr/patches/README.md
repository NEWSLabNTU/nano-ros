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
