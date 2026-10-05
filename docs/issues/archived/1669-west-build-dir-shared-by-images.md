---
id: 1669
title: "`nros build` hands west no `--build-dir`, so every Zephyr image of a workspace configures into `<ws>/build` — the directory nros keeps its own per-coordinate trees in — and a second image is refused"
status: resolved
resolved_in: 2026-10-05
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

## Resolution (2026-10-05)

Fixed in the PR that carries this section. Stage 4's west arm passes
`-d <ws>/build/<coordinate>/<bringup>__<image>_west` in west's first argument
zone, before the application path (`-d` takes one value, so it cannot swallow
the positional), unless the caller's passthrough already names a build dir
(`-d`, `-d<dir>`, `--build-dir[=]`) -- the fixture lanes pass one, and theirs
wins. One spelling, `cmd::build::west_build_dir`. Keyed on the BRINGUP as well
as the image id because two bringups may legally declare one id over one
hand-written application (`realtime-c`); the application is shared, the build
tree must not be. `<bringup>__<image>` is the spelling `resolved.toml`'s
directory already uses.

Test-side locators: none named a `nros build` west tree -- the fixture lanes
(`zephyr-fixture-run-one.sh`) always pass their own `-d`, so their build-dir
names (issue 1016's routing key) are unchanged, and no `zephyr/zephyr.*` path
literal was added, so `check-west-leaf-vocabulary` has nothing new to harvest.
The book's artifact table (`build-artifacts.md`) and the Zephyr quick-start's
run line now name the per-image path.

Measured (acceptance), `examples/workspaces/cpp`, this checkout's own Zephyr
workspace (`cp -al`, issue 1280), census taken first for both images:

| step | result |
| --- | --- |
| `nros build zephyr` | rc 0 -> `build/zephyr-zenoh/demo_bringup__zephyr_west/zephyr/zephyr.exe` |
| `nros build zephyr_cyclonedds` (no `--pristine`) | rc 0 -> `build/zephyr-cyclonedds/demo_bringup__zephyr_cyclonedds_west/zephyr/zephyr.exe` |
| `nros build zephyr` again | rc 0 |
| `<ws>/build/CMakeCache.txt` | absent |

Test: `build_verb_pipeline::a_west_image_builds_in_its_own_dir_unless_the_caller_names_one`
(the default dir is on the line, `<ws>/build` is not; a caller's `-d` wins and
is the only one) -- red with `cmd/build.rs` reverted (the line had no `-d`).
