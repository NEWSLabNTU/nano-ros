# phase-555 W3: the design-review rulings

This is the consolidated decision list the user ruled on, collated from the
seven area briefs (`phase-555-w2-*.md` in this directory). Its item ids (A1,
B1-B30, C1-C14, D1-D3) are THIS list's. Each brief numbers its own items; the
bracket at the end of each item names the brief and its item id, and is the
way back.

## Rulings (2026-11-14, relayed by the manager session)

The user ruled "Accept the rest of design review": every item takes its
recommendation below, except where an earlier user ruling decides it.

| item | ruling |
| --- | --- |
| A1 | (a): a doc becomes Stable only after its open questions and parked work are filed as issues and the doc points at them. |
| A2 | Land RFC-0107 as Stable, amended by RFC-0108 (done, #3755). |
| B1-B30 | As recommended. |
| C1 | Refuse at build time plus document (earlier ruling; merged). |
| C2 | (b): keep "builds with plain cmake" for C/C++, drop it for Rust (earlier ruling, consistent with the unified-CMake direction). |
| C3-C9, C11, C12, C14 | As recommended. |
| C10 | A Humble+Jazzy lane; the Iron claim is dropped; Jazzy stays "codegen-checked" until the lane is green (earlier ruling). |
| C13 | Graph services are ON by default, with a per-image off switch, per the default-services ruling. NOT the recommendation's "opt-in per image". |
| D1-D3 | Unblocked: the RFC-0130 CMake road has merged. |
| Untracked items | Filed: issues 2522 (CAN hardware validation), 2523 (bridge converter key), 2524 (nros-cpp CDR-header sniff), 2525 (vtable header section markers), 2526 (uORB README, fixed and resolved). |

## Who applies what (W3)

Each area's W2 reviewer applies its own items.

| area | owner | items here | the brief's own ids |
| --- | --- | --- | --- |
| rmw-wire | 139 | B1, B2, B9-B17, A1 | B1-B18 |
| build-toolchain-codegen | 241 | B3, B18, B19, B28, B29, C2, C3, D1-D3, A1 | B1-B16 |
| platform-board-rtos | 118 | B20, C4-C7, A1 | P1-P7 |
| workspace-system-model | 142, with 133 | B8, B23, B24, C8, C9, C11, A1 | items 1-8 |
| test-ci | 175 | B27, B30, C10, A1 | items 1-5 |
| runtime-api | wtf | B4, B5, B21, B22, C12, C13, A1 | Q1-Q7 (Q8 done) |
| docs-ux-diagnostics | AUO book, with 243 | B6, B7, B25, B26, C14, A1 | N1-N4, D1-D2 |

## A. Cross-area (rule these first; each one decides items in several areas)

A1. When a design doc's work is fully built and its phase is closed, does the doc
move from Draft to Stable?
- Options: (a) Stable only when it has no open questions and no parked work,
  so open items are filed as issues first and the doc then points at them;
  (b) Stable whenever its phase is done, even with open follow-ups;
  (c) leave every status alone until the index cleanup normalises them.
- Recommendation: (a). It reconciles the two reviews that disagree: both want
  the flip, and the disagreement is only whether open questions must be filed
  first. Filing them first is cheap and keeps Stable meaning something.
- About 40 docs in all seven areas follow from this one answer, so the per-area
  "flip to Stable?" questions are not asked again below.
  [platform P4; workspace 4; build B8; runtime Q1; test-ci 4; rmw-wire B10-B15; docs D2]

A2. The shared-executor design doc is cited by 18 source files and amended by a
later doc, but it was never merged. Land it as Stable, with a note that the
later doc amends it, or fold its surviving decisions into the later doc and
rewrite the 18 citations?
- Recommendation: land it. One file restores the target the code already
  cites. Its status line is updated to what was built.
  [runtime Q8; RFC-0107/0108]

## B. Settled: the recommendation follows from the code or an earlier ruling; just confirm

Archive or supersede (each doc's text is now wrong, and a newer doc owns the subject):

B1. Archive the old crate-layout doc, moving its one live section
(receive-buffer sizing) into the RMW slot-contract doc. [rmw B1; 0005]
B2. Supersede the old RMW/platform interface doc by the "C headers are the
source of truth" doc. [rmw B2; 0006]
B3. Archive the finished "type description subtype" doc. Its work landed, and
its second path was never needed. [build B5; 0030]
B4. Supersede the old C/Rust API surface catalogue: the generated and gated
API-parity ledger replaced it. [runtime Q2; 0037]
B5. Merge the 2026-04 C API audit doc into the C API wrapper-rules doc, and its
method into the periodic audit checklist. [runtime Q5; 0020]
B6. Supersede the old ROS 2 user-workflow doc by the workspace-first layout doc.
[docs N1; 0027]
B7. Move the zonal-vehicle market survey out of the design series into
`docs/research/`. It decides nothing. [docs N2; 0029]
B8. Archive the second phase-212 workspace layout doc, and cut the first one
down to the rationale that code still cites. [workspace 1; 0024/0025]

Amend to match what was built (no design change, the text just lags):

B9. The data-driven bridge runner is the design. Record its defaults
(forward every declared topic, one-way unless asked) and delete the unused
generated-bridge path. [rmw B3; 0009]
B10. Rewrite the PX4 backend doc for the C++ backend that ships, keeping the
Rust design as history. [rmw B4; 0011]
B11. The declared-to-lowered rule doc: strike the lowering mechanics two later
docs retired, and link the amendment both ways. [rmw B6; 0031]
B12. The RMW vtable doc: drop the never-built `abi_version` field. Record "one
checkout is one ABI, held by three gates; a slot nothing calls may be
removed". [rmw B7; 0035]
B13. In-place receive: record it as per entry point (2 of 11 today), not the
default. Record the pooled receive as deferred, and file an issue for the
borrowed-view case. [rmw B8; 0038]
B14. Serialization encoding: XCDR1 FINAL everywhere. Record "default
Appendable" as refuted and the per-type signal as future work. [rmw B9; 0055]
B15. CAN ISO-TP: narrow the claim to Linux and Zephyr, and file the MCU ports
as future work. [rmw B16; 0083]
B16. Serializer format ids: record "authored per provider, unique in the tree",
and file an issue for the unbuilt bridge converter key. [rmw B17; 0088]
B17. Endpoint provenance in `nros config explain`: move it to "not decided
here" unless a phase picks it up. [rmw B18; 0109]
B18. The boot-scaffold / multi-task doc: rewrite its banner and point its
retired sections at their new homes. It stays the task contract.
[build B6; 0032]
B19. Scaffolding-and-crates doc: one decision stands, one points at the
generated cargo config, and a false clause is deleted. It stays Draft until
the store-resolved install exists. [build B7; 0040]
B20. Where a configuration value comes from (RTOS knobs): amend in place. The
knob registry now GENERATES the Kconfig fragment. The precedence ladder
stands. [platform P2; 0049]
B21. C++ API doc: cut the sections that use deleted names, and keep its
freestanding / no-exceptions / FFI constraints. [runtime Q3; 0018]
B22. C wrapper structs: drop the never-built "state + opaque only" target.
Mirrored registration fields are accepted. [runtime Q4; 0019]
B23. Platform descriptors: two platforms have no crate, so their descriptors
stay in `config/`. Say so in the doc. [workspace 5; 0086 D3]
B24. Generated-path table: amend it to the files the code really writes.
[workspace 7; 0103 D7]
B25. CLI verb table: record the shipped tree (`explain` and `update`
top-level). [docs N4; 0117]
B26. Adoption-purposes doc: replace four sections whose bodies contradict
their own banners with pointers to where each now lives. [docs N3; 0116]
B27. Test ladder doc: split "when" into "run locally when" and "CI runs it".
State that no runtime rung gates a merge. [test-ci 1; 0061]
B28. Contradiction fixes, no choice involved: the extension SDK is the one
`nros*` crate allowed on crates.io; the root `[overrides]` fallback is
recorded as retired; the leaf-lockfile gate stays; and one sentence explains
"store first" vs "store last" (different subjects). [build B9-B12]
B29. Close open questions the code already answered, one dated line each, and
make the stale-text amendments listed in the build brief. [build B14, B15]

Already landed; confirm only:

B30. The tier-1 host pool is derived from existing board fields (host, witness,
automated cadence) rather than a new field. It landed with the cadence change.
[test-ci 2, 3; PR #3665]

## C. Needs a real choice

C1. E2E message protection works only on zenoh. On Cyclone and XRCE an image
built with it reports every sample valid without checking anything. Options:
(a) document it as zenoh-only; (b) refuse the feature at build time on a
backend that cannot carry it; (c) design a DDS carrier, which the doc earlier
rejected for interop.
Recommendation: (b) plus (a). Today the unsupported case says "valid", the
worst of the three. [rmw B5; 0028]

C2. Copy-out examples: is "builds with plain cargo/cmake, no `nros`" still a
promise? Options: (a) drop it everywhere; (b) keep it for C/C++ (an ordinary
ROS CMake package) and drop it for Rust, which needs generated config.
Recommendation: (b). The C/C++ half waits on the CMake-package change below.
[build B4]

C3. The per-image application named by a `DEPLOY` line inside a package:
retire it, or keep it as an escape hatch for hand-written Zephyr apps?
Recommendation: keep it as an escape. A hand-written west app is an
application, not a node package, so the earlier "a package names no board"
rulings are not broken. Four test fixtures use it. [build B13; 0085 D4]

C4. The RTOS-as-host principle is stated twice, in an old doc and a newer one.
Merge the old one's surviving parts into the newer and supersede it, or keep
both, with the old one amended?
Recommendation: merge. [platform P1; 0003/0072]

C5. The embedded-framework / board-key doc has no phase. Options: open a phase
for its seven steps; archive it; or fold it into the tier doc's last decision.
Recommendation: open a phase, with that decision as one of its items. The
RTIC dispatch-lint gap is a real defect. [platform P5; 0111]

C6. The support-tier doc after the Tier 0 withdrawal: amend in place;
supersede it by a short doc holding the two live decisions; or merge those two
into the newer doc.
Recommendation: amend in place, and check RTIC against criterion 5.
[platform P6; 0112]

C7. The long-lived exploration doc that still says LIVE although its phases
closed. Options: close it as Stable and file its open steps; split it into a
short Stable stance plus an archived exploration log; or keep it live and open
a phase for steps 6 and 8.
Recommendation: split, and file issues for the open steps. [platform P7; 0064]

C8. Topic visibility and locality has decisions but no phase. Open the phase
now with the opt-in tier first (it needs only one image key and zenoh-pico's
existing filter), or wait for the launch-grammar release?
Recommendation: open now. The grammar-dependent items wait behind that
release. [workspace 8; 0125]

C9. `nros-log`'s deprecated pick-one features were promised for one release,
and a release has shipped. Remove them now (with a build error naming the
replacement knob), or keep them one more release?
Recommendation: remove now and keep the refusal, the same rule as retired knob
names. [workspace 6; 0086 D5]

C10. Multi-edition interop: the support table says Iron and Jazzy are
"Supported", and no CI job runs the edition harness. Options: (a) add a
weekly hosted lane; (b) lower the claim to "codegen-checked"; (c) leave it.
Recommendation: (a), with (b) until the lane is green. [test-ci 5]

C11. The configuration-homes doc (Stable) and the copy-out canonical-shape doc
describe the standalone-package shape that phase-531 is migrating away from.
Restate them now, or once, when the migration's last step retires that shape?
Recommendation: once, after the migration, with a note now on what moved.
[workspace 2, 3; 0004/0026]

C12. Zero-copy follow-ups (raw-bytes examples, bench, book chapter, Cyclone
loans) are in no phase. Open one, or record them as not planned?
Recommendation: not planned, until someone needs measured zero-copy on DDS.
[runtime Q7; 0010]

C13. ROS divergences: should the doc list the graph services nano-ros leaves
off by default now, or from phase-556's measurement as one rule ("graph
services are opt-in per image")?
Recommendation: from phase-556, as one rule. [runtime Q6; 0036]

C14. The two-books doc and the book-charter doc overlap. Keep both, or fold the
first into the second?
Recommendation: keep both. They cover two subjects (the split vs the user
book), and merging breaks links that gates cite. [docs D1; 0123/0124]

## D. Wait on the CMake-package change (PR #3634); confirm the plan now

D1. When that change lands, supersede the per-package `deploy board rmw` doc by
it. [build B1; 0048]
D2. Then rewrite the build-road table as one current table and mark it Stable.
[build B2; 0065 D3]
D3. Then add a banner to the two passages that still describe cargo-built
bare-metal C/C++ leaves. Striking them is a phase-553 item. [build B3; 0098/0101]

## Untracked work found (filed)

- CAN hardware validation is in no open phase: issue 2522.
- The bridge converter key is unbuilt (B16): issue 2523.
- An unconditional CDR-header sniff in the C++ action code, probably
  unreachable: issue 2524.
- Section markers in the RMW vtable header (it regenerates the table and
  touches the C ABI header): issue 2525.
- The stale uORB backend README: issue 2526, fixed and resolved.
