---
id: 1669
title: "`nros build` hands west no `--build-dir`, so every Zephyr image of a workspace configures into `<ws>/build` — the directory nros keeps its own per-coordinate trees in — and a second image is refused"
status: open
type: bug
area: [build, cli, zephyr]
severity: medium
found: 2026-10-03
related: [0892, 1288, 1653, rfc-0065]
---

## What

Stage 4's west arm (`cmd/build.rs`, `Driver::West`) composes
`west build -b <board> <app> -- <cmake args>` and runs it from the WORKSPACE
root. It passes no `-d` / `--build-dir`, so west uses its default: `build/`
under the current directory, i.e. `<ws>/build` — the same directory `nros sync`
and `nros build` fill with `build/<coordinate>/…`, `build/nros/models/…` and
`build/<bringup>__<image>/resolved.toml`.

Measured 2026-10-03 on `examples/workspaces/cpp`:

1. `nros build zephyr_cyclonedds` configured Zephyr straight into
   `examples/workspaces/cpp/build/` (a `CMakeCache.txt`, `zephyr/`,
   `modules/`, `build.ninja` beside nros's own coordinate directories).
2. `nros build zephyr` (the zenoh image of the same bringup) then failed before
   configuring:

   ```
   ERROR: Build directory ".../examples/workspaces/cpp/build" is for application
   ".../build/zephyr-cyclonedds/zephyr_cyclonedds_entry", but source directory
   ".../build/zephyr-zenoh/zephyr_entry" was specified; please clean it, use
   --pristine, or use --build-dir to set another build directory
   ```

So two Zephyr images of one workspace cannot both be built by `nros build`,
and the first one's build tree is mixed into nros's own `build/` root.
Passing `-- -d build/zephyr-zenoh/west` by hand works (the measurements for
issues 1653 / 1649 used it).

## Direction

Give each west image its own build directory by construction, beside the
generated application it already has (`build/<coordinate>/<image>_entry/`) —
e.g. `build/<coordinate>/west` — and pass it as `-d` in west's FIRST argument
zone (before the application path, unlike `-p`; see the `--pristine`
comment in the same arm). A user-supplied `-d` in the passthrough must still
win. Then the fixture lanes and any locator that names a west build dir for a
`nros build` image need to agree on the one spelling (issue 1016's rule:
a west leaf routes by build-dir NAME).

## Acceptance

`nros build zephyr` and `nros build zephyr_cyclonedds` of
`examples/workspaces/cpp` both build, back to back, with no `--pristine`, and
neither writes a `CMakeCache.txt` at `<ws>/build/`.
