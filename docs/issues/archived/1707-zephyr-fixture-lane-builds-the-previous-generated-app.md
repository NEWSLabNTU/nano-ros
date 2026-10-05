---
id: 1707
title: "The Zephyr fixture lane builds an already-configured image from the PREVIOUS generated west application: a declaration change arrives one lane run late"
status: resolved
resolved_in: 2026-10-06
type: bug
area: [zephyr, build, testing]
severity: medium
found: 2026-10-06
related: [1702, 1681, 1288, 0196]
---

## What was measured

`nros build` generates each Zephyr image's west application under
`<ws>/build/zephyr-zenoh/<image>_entry/` (phase-470 W5). That includes
`NANO_ROS_FEATURES` and, since issue 1702, the `CONFIG_NROS_CAPABILITY_*`
Kconfig assignments. `scripts/build/zephyr-fixture-run-one.sh` re-runs an
already-configured leaf with plain `ninja -C <build_dir>` whenever its record
signature is unchanged, and editing `system.toml` does not change that
signature.

In `just zephyr build-fixtures` on `examples/workspaces/features`, every
generated application's `CMakeLists.txt` was rewritten while the run was
already underway, AFTER some leaves' `ninja` had started or finished. So the
images built from the previous file. Two independent sightings:

1. **Capability Kconfig (issue 1702 measurements).** After
   `features = ["param_services"]` became `[]`, one lane run left:

   ```
   cpp-params: app[05:56:15] PARAM_SERVICES n | build.ninja[05:56:18] .config: CONFIG_NROS_CAPABILITY_PARAM_SERVICES=y
   rs-params:  app[05:56:15] PARAM_SERVICES n | build.ninja[05:51:41] .config: ...=y
   ```

   The cpp leaf's ninja-triggered reconfigure wrote `extra_kconfig_options.conf`
   with the OLD `y`, because the cache variable the new file sets was not
   defined during that configure. The Rust leaves never reconfigured, because
   their ninja ran before the rewrite. A plain `cmake <build_dir>` afterwards
   applied the new value at once.
2. **`NANO_ROS_FEATURES`.** After `[image.zephyr_cpp_params]`'s bringup dropped
   `param_services` (keeping `lifecycle`), the image built nros-cpp with
   `param-services,...` and no `lifecycle-services`, which is the previous
   run's axes, and failed to link
   (`undefined reference to nros_cpp_lifecycle_autostart`). The next run, with
   the axes restored, failed the other way
   (`undefined reference to nros_cpp_register_parameter_services`). A
   `cmake <build_dir>` re-configure gave the right feature string both times.

So a link failure or a wrong heap here can be a museum configure rather than a
code defect. A fresh build dir is unaffected (it takes the `nros build` path,
which writes the application BEFORE configuring), which is why CI does not see
it.

## Root cause (measured)

Two missing edges, one on each side of the configure.

1. **The lane's ninja path never regenerated the application.** Only stage 4
   of a `plan_builds` writes it. `zephyr-fixture-run-one.sh` runs one on the
   west path (`nros build`), and its ninja path, which a matching signature
   takes, ran none. The signature does not cover `system.toml`.
2. **The rewrite that did happen landed inside a configure, after cmake read
   the file.** `nros_check_image_agreement` (`cmake/NanoRosImageAgreement.cmake`)
   runs `nros image-facts --for-entry`. That query plans the WHOLE workspace
   (`all: true`), and `plan_builds` writes every image's application. So any
   leaf's configure rewrote every sibling's application (the "rewritten while
   the run was already underway" in the first report), and its own. Its own
   rewrite came after cmake had read the old file. The Ninja generator writes
   `build.ninja` after that, so `build.ninja` ends up newer than its input.
   With ninja 1.10 (this host), no later `ninja` reconfigures.

Reproduced on `examples/workspaces/features` `[image.zephyr_rust_qos]`
(native_sim/native/64), with the code as it was before this fix:

| step | application says | `.config` |
| --- | --- | --- |
| lane build, `features = ["param_services"]` | `y` | `PARAM_SERVICES=y`, heap 524288 |
| `features = []`, lane again (ninja path) | `y` (never regenerated) | `y`, no reconfigure |
| `cmake <build_dir>` | `n` (rewritten mid-configure) | `y` |
| `ninja` | `n` | `y`, no reconfigure; `build.ninja` 2 s newer than the app |

## Fix

- **Edge 1.** The runner's ninja path runs `nros build <image> --workspace <ws>
  --offline --dry-run` before `ninja` for a retargeted row. That command runs
  stages 1 to 4 and execs nothing, in about 10 ms. Every write is
  write-if-changed. So an unchanged declaration leaves the application's mtime
  alone and ninja does not reconfigure. A changed one makes it newer than
  `build.ninja`, and ninja's RERUN_CMAKE edge reconfigures from the new file.
- **Edge 2.** The generated application states a fingerprint of its own body,
  `set(NROS_GENERATED_APP_DIGEST <sha256>)`.
  `nros_check_generated_app_current`, deferred to the end of the top-level
  directory, compares the digest this configure EXECUTED with the line now on
  disk. On a mismatch it refuses with `FATAL_ERROR`. Nothing is generated, so
  the old `build.ninja` stays older than the new file and the next build
  reconfigures from it. A digest has no window. An mtime or a before/after hash
  taken inside the configure would have one, because the configure is already
  running when the module loads.

## Measured after (same image, this fix)

| step | application | `.config` | reconfigured |
| --- | --- | --- | --- |
| lane, unchanged declaration | `y` | `y`, 524288 | **no** (`build.ninja` mtime unchanged) |
| `features = []`, lane ONCE | `n` | `PARAM_SERVICES` not set, heap **196608** | yes |
| `features = ["param_services"]`, `cmake <build_dir>` | `y` | `FATAL_ERROR` (issue 1707) | |
| then `ninja` | `y` | `y`, 524288 | yes |
| `cmake <build_dir>` with the file current | | rc 0 | |

The C++ arm, `[image.zephyr_cpp_params]`, gives sighting 2 in one lane run
each way:

- `features = []`: `NANO_ROS_FEATURES "lifecycle"`, nros-cpp rebuilt. The image
  links with `nros_cpp_lifecycle_autostart` and without
  `nros_cpp_register_parameter_services`. Heap 196608.
- Back to `["param_services"]`: both symbols are present and the heap is 524288.
- An unchanged rerun does not reconfigure.

Test: `packages/testing/nros-tests/tests/zephyr_fixture_app_regen.sh`
(`just check zephyr-app-regen`, fast line) asserts the runner order (R1), runs
two negative controls (R2, R2b), and checks the configure side with `cmake -P`
(C1 refuses, C2 unchanged passes, C3 plain app untouched). Against the old
runner and cmake, R1, C1, C2 and C3 fail. The renderer's digest is pinned by
`west_app::tests::the_application_states_its_own_digest`.

## What this does not change

`nros image-facts` still writes generated files as a side effect of a query.
Its module docs say it "produces no artifacts". Edge 2 makes that harmless for
the configure that runs it, and for a sibling it is an ordinary early
regeneration. Making the query side-effect free would mean threading a no-write
mode through `generate_entry` and every writer it reaches. That change is not
needed for correctness here.
