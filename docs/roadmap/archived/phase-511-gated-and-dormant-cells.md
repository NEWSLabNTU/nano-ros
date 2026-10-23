# Phase 511 (B/C) — cells for the six tests with blockers: a Gated tier, a Dormant row, and one interop row

**Status (2026-10-23). DONE.** B1–B4 and C landed, and every test this phase
modelled has passed in its own lane. B2's test first failed there on a real
defect (issue 2003), fixed in this phase. C's live verdict is recorded (pass).

Implements RFC-0051 §7c (amendment 2026-10-23, B/C). It follows
[phase-511 A/D](phase-511-unmodeled-tests-to-cells.md), which left six
binaries `NonCell::Unmodeled`, each with a stated blocker.

## What each needed

| | Test | Was | Now |
| --- | --- | --- | --- |
| B1 | `freertos_qemu` | Unmodeled: "mps3-an536 is not a platform value" | `(FreertosMps2, Cpp, Cyclonedds, EntryPubsub, Workspace)` BuildOnly → **Runtime** |
| B2 | `fvp_runtime_ws` | Unmodeled | `(Fvp, Cpp, Cyclonedds, RealtimeTiers, Workspace)` **Gated** |
| B2 | `fvp_smoke` | Unmodeled | `NonCell::Probe`: a printk board-import boot, no RMW |
| B3 | `px4_xrce` | Unmodeled: "Px4 is modelled only as uORB" | `(Px4, Rust, Xrce, Pubsub, Example)` **Gated** |
| B4 | `esp32_emulator` | Unmodeled | `NonCell::Dormant(Esp32Qemu)` + a gate |
| C | `cyclone_slirp_pair` | Unmodeled | interop row `native-slirp-pubsub-cpp-cyclone-n2r`, in `SLIRP_PAIR_CELLS` |

- **B1.** The board was never the obstacle. The W6 interop cells already put
  mps3-an536 under `FreertosMps2` (the `freertos` family token). The coordinate
  was BuildOnly only for the S32Z270 row, which shares it and cannot boot. With
  an emulated twin as witness, the cell no longer waits on S32Z270 hardware.
  That row is still built.
- **B2, B3: `Tier::Gated(lane)`, new.** A runtime test exists and binds its cell,
  but the prerequisite is one no CI runner provisions, so it runs only in the
  lane the string names:
  - FVP: the model, `nros setup --tool arm-fvp`.
  - PX4: the PX4-Autopilot `.msg` tree.

  `Runtime` would put either in tier 2's platform cover (0232's false green);
  `BuildOnly` would deny a test exists. The supporting changes:
  - `answered_cells` (Runtime ∪ Gated) is `consumer_of`'s domain.
  - `runtime_cells` stays the covers' domain.
  - `platform_is_built` excludes Gated, in both its Rust and Python readings.
- **B4.** ESP32 was already modelled: every Esp32Qemu cell is
  `CarveOut(ESP32_DORMANT)`. What was missing was a name for the TEST.
  `dormant_rows_name_a_platform_with_no_answered_cell` makes the row false the
  moment a cell revives.
- **C.** The guest is the Cyclone backend's C++ `ros2_pub`, so the coordinate is
  `(Linux, Cpp, Cyclonedds, Pubsub)`. The row needs a focused runner
  (`just linux test-ros2-cyclone-slirp-pair`), which `check-interop-cell-runners`
  demanded.

## What running them found

Modelling put four tests in lanes for the first time, and three of those lanes
were broken in ways nothing had reported.

1. **`px4_xrce`: [issue 2002](../../issues/archived/2002-px4-qos-profile-refused-by-xrce.md), fixed here.** Since issue 1329 (2026-09-11) the
   XRCE backend refuses liveliness, which it cannot honour. `QOS_PROFILE_PX4`,
   the profile written for that backend, still demanded `Automatic`, so every
   companion failed to advertise its first topic
   (`IncompatibleQos(QoSPolicyMask(128))`). It now carries the sentinel; 3/3 pass.
2. **The FVP recipes: issue 0698's rule had not reached them.** Zephyr 3.7
   expands `ZEPHYR_TOOLCHAIN_VARIANT` unquoted, and CMake 4 refuses the
   configure when it is unset. Three callers named it; five recipes did not:
   - `build-fvp-ws-entry`
   - `run-fvp-ws-entry`
   - `build-fvp-board-import`
   - `run-fvp-board-import`
   - `zephyr-dev`'s `build-c-port`

   All five now use `nros_zephyr_toolchain_variant`. `check-zephyr-sdk-env` gains
   the reach check, with a negative control; run over the pre-fix recipes, it
   names exactly those five.
3. **`resolve-fvp-bin.sh` depended on its working directory.** It asks
   `nros sdk-path`, which finds the SDK index from the cwd, and a cargo test runs
   in its crate dir. Both FVP tests reported "not resolvable" over an installed
   model. The resolver now asks from the checkout root.
4. **`fvp_runtime_ws`: [issue 2003](../../issues/archived/2003-fvp-cyclone-lifo-misaligned-by-8-byte-heap.md), fixed.** Once it could run, the image
   data-aborted on both tier CPUs right after `dds_create_participant`: Cyclone's
   16-byte lock-free LIFO landed in an 8-aligned slot of `zpico-alloc`'s slab,
   and AArch64 `caspal` faulted. The arena now guarantees
   `2 * size_of::<usize>()` (16 on 64-bit), which costs no padding: it is
   rlsf's own payload alignment, and only the storage's `align(8)` had broken
   it.

Also corrected, because it was the premise of B2's tier question: the matrix's
FVP comments and BuildOnly reasons called the model "license-walled
(`[gated.arm-fvp]`)". `nros-sdk-index.toml` has said otherwise since 2026-09-06:
a public download, x86_64 only.

## Measured

- `just ci gate`: see the PR.
- **Lane covers:**
  - Tiers 1 and 2: unchanged (23/11, 11/10).
  - Nightly: 39/37 → 37/35, with every declared pair still covered.
  - Tier 3: 247 cells over 51 coordinates.
  - The Gated cells are in no cover.
- **Runtime:**
  - `freertos_qemu`: pass, in 103 s.
  - `px4_xrce`: 3/3 pass after the issue 2002 fix.
  - `fvp_smoke`: pass.
  - `fvp_runtime_ws`: failed (issue 2003); after the fix, 2/2 pass on a
    rebuilt image, both tiers publishing.
  - `cyclone_slirp_pair`: 2/2 pass in 37 s (`just linux test-ros2-cyclone-slirp-pair`,
    Humble, once `slirp4netns` was installed); verdict recorded in
    `.config/interop-verdicts.toml`.

## Remaining

None. The FVP cell stays `Gated` (maintainer ruling 2026-10-23): it gets
no CI runner and no scheduled lane. It runs when someone types
`just zephyr verify-fvp-runtime`. The rejected options were adding the
model (a public x86_64 download, `nros setup --tool arm-fvp`) to the tier-2
image as a Runtime cell, and a scheduled lane that runs only that recipe.

## Related

- RFC-0051 §7c; RFC-0061 (re-measured 2026-10-23).
- Issues 0232, 0698, 1251, 1329, 1525, 2002, 2003.
