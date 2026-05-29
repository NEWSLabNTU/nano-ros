# Zephyr per-line patch sets (Phase 199.3)

`just zephyr setup` applies a **version-dispatched** patch set:

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
- Every patch must be **idempotent** (`just zephyr setup` re-runs them).
- Most patches edit Zephyr *internals* (native_sim/NSOS — not stable APIs); these
  are the churn Phase 199 tracks for upstreaming. As fixes land upstream, drop
  the corresponding line so the carried set shrinks per release.
