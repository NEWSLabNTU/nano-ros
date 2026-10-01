---
id: 1610
title: "The Zephyr adapter shim crossed its 200-LoC budget, red for three days
  in a lane nothing ran — and the 45 lines that crossed it were the ONLY place
  Cyclone's domain was checked, so the entry road reported a split-brain as
  agreement"
status: resolved
type: bug
area: build, zephyr, ci
severity: high
found: 2026-10-01
resolved: 2026-10-01
related: [0161, 0922, 1226, 1423, 1508, 1550, phase-212, phase-460, RFC-0003, RFC-0049]
---

## What was reported

`loc_budgets::rtos_adapter_shims_under_200_loc_each` failed on `main`:

```
Phase 212.H.8 adapter-shim 200-LoC budget violated:
  - zephyr (zephyr/cmake/nros_system_generate.cmake): 208 LoC > 200
```

Measured by bisecting the file's own history with the same counting rule tokei
applies (non-blank, non-`#` lines), which reproduces tokei's 208 exactly:

| commit | date | LoC | |
| --- | --- | --- | --- |
| `379f7c8d2` (#1312) | 09-18 | 140 → **198** | the large jump, +58 |
| `7ef585b6b` (phase-460 W4) | 09-21 | 198 | |
| `d27bce139` (#1508) | 09-28 | **203** | **crossed** |
| `b6b763edb` (#1550) | 09-28 | 208 | |

## Why it stayed red — the lane

`test-unit` runs `--workspace --exclude nros-tests`, because that crate's tests
generally need staged fixtures. `loc_budgets` needs none — it counts source
lines through the `tokei` library — so it was reachable only from the fixture
lanes, and the fixture lane on `main` is `host-tests`, which has produced **no
green run in its last 200** (issue 1500). Nothing between `d27bce139` and its
merge asked the question.

This is issue **0922**'s defect — excluded by CRATE where the property is
per-TARGET — and issue **1226**'s shape, *a gate that works is not a gate that
runs*. The test's own header already said so about an earlier break: *"this
test has been RED since that commit; nothing noticed, because no merge-gating
lane runs it."* That sentence was written, and the lane was not changed.

## Why the budget mattered — the defect behind the number

**RFC-0003 §6 says what to do when a shim exceeds its budget, and it is not to
raise the budget:** *"split shell into shared core + per-board overlays … Don't
paper over by raising the budget."* So the question was what in the shim was
actually shared core.

The answer: `nros_system_check_domain_agreement`, ~45 code lines. Issue 1550
had already created the shared helper `nros_check_domain_agreement`
(`cmake/NanoRosDomainAgreement.cmake`) and the shim CALLED it — but kept its own
copy of the Kconfig comparison beside the call, plus one comparison the helper
did not have:

```cmake
if(DEFINED CONFIG_NROS_CYCLONE_DOMAIN_ID)
    if(NOT _cyclone EQUAL _baked) set(_agree FALSE) endif()
```

**The shared helper contained the word `CYCLONE` zero times.** So
`nros_system_generate` compared Cyclone's own domain knob and the ENTRY road
(`NanoRosEntry.cmake:634`, the other caller) did not. Measured against
`main`'s helper — a Cyclone image with `CONFIG_NROS_CYCLONE_DOMAIN_ID=2` and
everything else on 10:

```
nano_ros_entry(test): domain 10 agrees -- CONFIG_NROS_DOMAIN_ID from snippet
island-ethernet (...); system.toml 10
-- Configuring done
```

A Cyclone image on DDS domain 2, configured clean, **reporting agreement**.
This is issue **0161**'s phase-180 split-brain — *"silently ran every cyclone
image on domain 0"* — reachable again on one of the two roads, with a status
line asserting the opposite. Worse than no check, because a reader who saw
"agrees" had no reason to look.

That is the recurring failure this repository keeps meeting: **one comparison,
delivered to one of the two roads that need it.**

## Fix

1. **The Cyclone comparison moved into the shared helper**, so both roads make
   it. The shim's function shrank to the one thing only it can know — which
   number the BAKE wrote into `system_config.h` — and a call.
2. **Every diagnostic the shim's own copy carried, the helper now carries**, so
   moving the check cost no information: Cyclone is named in the refusal even
   when unset (`= unset` says it played no part, which absence cannot), the
   remedy names the concrete value (`CONFIG_NROS_DOMAIN_ID=10`, not `=<n>`),
   and the agreement line shows the Cyclone value it compared — otherwise a
   pass that never looked at Cyclone reads the same as one that did. The entry
   road gains all three.
3. **`loc_budgets` joined `test-lane-contracts`**, the merge-gating home for
   `nros-tests` targets that build no fixture. That recipe's admission rule is
   stated as a property of the TARGET (`gate.yml`: *"they resolve no fixture
   stamp and build no fixture"*) and `check-lane-contracts` enforces it, so
   this is the rule applied, not a recipe stretched. Its NAME is narrower than
   its rule, which the recipe comment now says.

Shim: **208 → 170 LoC**, under budget by 30 rather than by trimming to 199.

## Acceptance — measured

| | result |
| --- | --- |
| `loc_budgets` | 2/2 pass; Zephyr shim 170/200 |
| `test-lane-contracts` with `loc_budgets` in it | 30/30 |
| **negative control**: shim inflated to 210 | lane exits 100, names `zephyr … 210 LoC > 200`; restored byte-identical |
| `cmake-domain-agreement-tests.sh` (system_generate road) | 21/21 — one grep re-worded for the helper's label, **none removed** |
| `cmake-entry-domain-agreement-tests.sh` (entry road) | 21/21, two new cases F/G |
| **control**: F/G against `main`'s helper | **exit 1** — F configures clean and prints `domain 10 agrees`; this is the regression test, not a description of one |
| `check-lane-contracts` | OK |

## What this does NOT claim

A coarse pass with `check-lane-contracts`' own `resolvers_used()` reports 83 of
182 `nros-tests` targets calling no RUNTIME fixture resolver, and most of those
appear in no merge-gating lane. **That is a lead, not a measurement.** Calling
no runtime resolver is not the same as needing nothing: `rtos_e2e`,
`fvp_smoke`, `qemu_patched_binary` and `ros_editions_e2e` are in that set and
plainly need QEMU, an FVP or docker. Treating the classifier's output as "80
ungated tests" would be issue 1452's shape — a reach wider than the rule. The
per-target property issue 0922 named needs a classifier that knows about every
kind of precondition before anyone counts with it.
