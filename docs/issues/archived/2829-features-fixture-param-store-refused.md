---
id: 2829
title: "The features workspace's shared C/C++ configure is refused: one image declares parameters and 16 do not, so the tier-1 and pairwise fixture builds stop"
status: resolved
type: bug
area: [build, cmake, params, testing]
severity: high
found: 2026-11-22
related: [1600, 2663, 2768]
resolved: 2026-11-22
resolved_in: "fix(params, issue 2768): the store counts launch seeds, and a watched silent image declares zero"
---

## What fails

`just build-test-fixtures lane=tier1` (and the tier-2 pairwise nightly's
fixture build) stops in its linux stage while configuring
`examples/workspaces/features`, bringup `demo_bringup`, the shared
`posix-zenoh-linux-demo_bringup` cmake configure that links one runtime for 17
images:

```
CMake Error at cmake/NanoRosEntityFacts.cmake:529 (message):
  Error: the parameter store cannot be sized: the entity inventory REFUSED
  the contract's `params:` declaration (NROS_PARAM_DECLARATION_STATUS
  "refused" in .../features/build/posix-zenoh-linux-demo_bringup/cmake/nros/entity_inventory.cmake):

    this configure links one runtime for several images, and 1 declare
    parameters while 16 declare none (demo_bringup/c_custom_msg_listener_model.yaml, ...)

  Declare `params:` on every node in the image, or on none.
  Location: nros-cli-core/src/cmd/image_entity_facts.rs:126:22
```

The one image that declares is `linux_mixed_params` (the 17 entries named
by the configure's runtime sizing descriptor, minus the 16 the refusal lists
as silent).

The stage then aborts, so every later linux fixture row is unbuilt too: in a
tier-1 run on top of it, 9 features rows and 22 other rows fail as
"Test fixture binary MISSING for an in-lane coordinate".

## Where it was seen

- Reproduced alone on main `047d83c28d` (host 142, 2026-11-22):
  `scripts/build/workspace-fixtures-build.sh linux c --id
  workspace-features-c-params` fails with the error above in 460 s.
- The 2026-11-21 scheduled `run-matrix` run (main at `f53e7ad71`): `tier 2
  nightly (pairwise cover)` stopped on it, so the nightly reached no cell
  (which is why issue 2663's "Done when" is still unmet).
- PR #4583's tier-1 run (base `ab2e78b522`), which touches only nros-tests.

## What is known

The refusal is issue 1600's rule (`entity_inventory.rs`,
`ParamDeclarations::Refused`, since `528bcebd75`): a multi-image configure
whose images disagree about declaring `params:` is refused rather than sized
from the declaring images, which would be short for the others. The rule is
older than the failure, so what changed is that `linux_mixed_params` now
counts as declaring. Its parameters are set in
`demo_bringup/launch/mixed_params.launch.xml`; a recent change to how a
launch parameter or a census-recorded parameter reaches the inventory is the
first place to look. Not yet bisected.

## Cause and fix (2026-11-22, nros 175)

`9fecdfd146` (issue 2768) made parameters a node declares in its code count
as a declaration, so `linux_mixed_params` began to declare while its 16
siblings stayed silent. Two gaps then met issue 1600's rule: the union read
an image the probe watched declaring nothing as unknown rather than as
declaring none, and a launch-file seed was never counted, so once the
configure got through, the C++ params cell refused its own `flags` seed at
boot. Fixed by #4602 (`2ad43f7873`, merged 2026-11-22).

## Done when

`just build-test-fixtures lane=tier1` builds the features rows on main, and
the next pairwise nightly's fixture build passes its linux stage. Either the
features configure states parameters for every image, or none, or the
inventory's rule is revised for a configure where only some images use
parameters, with the store still sized for every image.
