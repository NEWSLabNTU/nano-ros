---
id: 1839
title: "Since phase-485 W8, both book probe tracks stop at configure: the book's setup flow never provisions the pinned Corrosion, and a configure no longer fetches it"
status: open
type: bug
severity: medium
area: [book, ci, cmake, provisioning]
related: [1439, 1304]
found: 2026-10-12
---

## Measured

Scheduled `probe` run 38224056082 (2026-10-12 08:31Z, head `b6e9ad52c`)
failed both jobs at the CMake configure:

- `book probe — installed track`, job 114727086556:

  ```
  nano-ros: the pinned Corrosion is not in the SDK store, and a configure
  does not fetch it (RFC-0104 D10).
    Install it:  nros setup --tool corrosion
  Call Stack (most recent call first):
    /root/.nros/sdk/nros/0.0.0-probe/share/nano-ros/CMakeLists.txt:111 (nros_resolve_corrosion)
  Error: configure failed: `cmake -S build/posix-cyclonedds-native -B build/posix-cyclonedds-native/cmake ...
  error: recipe `installed` failed on line 39 with exit code 1
  ```

- `book probe — checkout track`, job 114727086844: the same refusal at
  `/root/nano-ros/CMakeLists.txt:111`, after the book's `[setup]` steps
  report `setup complete for target=posix-zenoh` and the probe prints
  `-- nano-ros: no Corrosion at the pinned prefix (/root/.nros/sdk/corrosion/0.6.1-nros1)`.

The previous scheduled probe, run 38124199770 (2026-10-11 08:16Z, `2b6c3aedc`), was
green. The refusal arrived with `36c8e84931` ("feat(phase-485 W8)!: a configure
never fetches Corrosion; with none in the store it refuses", 2026-10-11 15:10).

## What it is

The refusal is working as designed (RFC-0104 D10). What it exposed is that
the book's documented first-project flow, which `just probe bootstrap` and
`just probe installed` execute block by block, never provisions Corrosion.
Before W8 the configure fetched it silently, so the flow never needed to.
The `[setup]` step on the checkout track sets up `target=posix-zenoh`
sources and reports complete without the tool a C/C++/mixed configure
needs. Only `vendored-tree.md` and `build-as-subdirectory.md` say to run
`nros setup --tool corrosion`; the getting-started path the probes run does
not.

So this is a book and user-flow regression, not a probe-harness bug: a new
user following the book on a fresh host hits the same refusal.

## What it is not

- Not a CI image or runner problem: both tracks run in clean containers,
  and the refusal names the one missing artifact.
- Not issue 1439 (checkout-track "rust entry exited before publishing") or
  1304 (installed-track cyclonedds gitlink). Both stop later than this, and
  this stops them from being reached.

## What would close it

The book's setup step provisions Corrosion. That can be an explicit
`nros setup --tool corrosion` block, or the board/target setup the book
already runs (`nros setup <board>`, which the refusal text says provisions
it) actually doing so for the probe's posix targets. It is proven by both
`book probe` tracks in a `probe` run getting past configure. Whichever
setup verb is meant to carry Corrosion should be the one the book shows,
so a user and the probe take the same road.

## Fix (in review)

`nros setup <board>` now carries Corrosion for every board whose platform
takes the CMake road. That is the verb the book already shows, so the
refusal's own advice is true and the book needs no extra step. The rule is
not a per-board list in the index. `road_build_tool` (`cmd/setup.rs`) asks
`builder::plan::driver_for(platform, true)`, the same question the builder
asks: `Driver::CMake` gets `[tool.corrosion]`, Zephyr (`west`) and esp32 (no
cross-language road) do not.

Measured in the ros2 box, with an empty `NROS_HOME`:

- `nros setup native --rmw cyclonedds --dry-run` lists `corrosion  source
  build 0.6.1-nros1`.
- The real run installs `<store>/corrosion/0.6.1-nros1`. The book's
  `examples/native/c/talker` configure then reports `Corrosion 0.6.1 via SDK
  store` and exits 0. Against an empty store the same configure still
  refuses, as RFC-0104 D10 intends.

Guarded by two `cmd::setup` unit tests, which run in `check-cli-tests` on
every pull request. One binds the rule against a synthetic index. The other
reads the shipped index and checks that `native --rmw <every rmw>` and every
CMake-road board plan Corrosion, and that no other board does.

The closing criterion is still open: both `book probe` tracks in a `probe`
run getting past configure.
