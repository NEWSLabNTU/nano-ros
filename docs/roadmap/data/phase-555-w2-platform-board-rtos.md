# Phase 555 W2 -- platform-board-rtos: verdicts and decision brief

**Status (2026-11-14). RULED AND APPLIED: the user accepted every
recommendation (A1 (a), P1 (a), P2 (a), P3 (a), P4 (c), P5 (a), P6 (a), P7 (b))
and phase-555 W3 applied them on 2026-11-14. What W3 did is under "W3 —
applied", at the end.**

The platform-board-rtos slice of
[phase-555](../phase-555-design-doc-review.md) W2. Reviewers: 118 (0003, 0012,
0013, 0016, 0017, 0034, 0039, 0049, 0126) and 133 (the rest). Each RFC
carries a dated "Review" section with the evidence; this page is the summary
and the questions for the user.

## Verdicts

| RFC | subject | verdict | changed in this review |
| --- | --- | --- | --- |
| 0003 | RTOS integration pattern | merge-into RFC-0072 (brief P1) | review note |
| 0012 | Board / BSP integration architecture | amend: dead crate names and paths; the counts and overlay crates now conflict with RFC-0040 and RFC-0064 | phase-139 link fixed |
| 0013 | Custom-board provisioning | archive. It is already Superseded by RFC-0087 and RFC-0062, and moves in W3 | back-links added on RFC-0087 and RFC-0062 |
| 0016 | RTOS scheduling features | amend: dead cmake rows; the banner misreads RFC-0004 | non-README frontmatter field dropped |
| 0017 | PlatformTimer and AtomicSporadicState | amend: record it as built (a C ABI timer, `AtomicU32`) | review note |
| 0034 | Platform layer split | amend the D6 Zephyr row, then consider Stable (brief P3) | review note |
| 0039 | PX4 integration | keep | review note |
| 0049 | Hierarchical platform and board config | amend (brief P2) | `amends: [rfc-0049]` added on RFC-0086 and RFC-0098 |
| 0126 | NuttX on NuttX's infrastructure | keep; RFC-0130 still has to land | review note |
| 0053 | ThreadX multi-sched-task execution | amend (133): the title and decision said static stacks; the code and its own revision use a byte pool | fixed in #3673 |
| 0064 | Board support organization | amend (133): still says LIVE though all its phases are archived; steps 6 and 8 never landed (brief P7) | review note |
| 0072 | RTOS integration: the RTOS owns its build | keep (133). Takes 0003 in under P1 | review note |
| 0073 | Platform clock | keep (133) | review note |
| 0076 | The platform ABI: a caller asks | keep (133) | review note |
| 0093 | A board name states its reach | keep (133) | review note |
| 0111 | An image's entry framework is its own field | keep (133); not started and has no phase (brief P5) | review note |
| 0112 | Frameworks through one host-driven contract | amend (133): Tier 0 withdrawn, D5 not done (brief P6) | status line fixed in #3673 |
| 0114 | Board support registry | keep (133) | review note |
| 0115 | Emulator-first board policy | keep (133) | review note |
| 0118 | Target execution model | keep. Reviewed by 139 AUO book in the docs-ux-diagnostics slice, which moved its area here ([phase-555-w2-docs-ux-diagnostics.md](phase-555-w2-docs-ux-diagnostics.md)) | area moved |

Tier 2 pairs, read in the newer RFC's own words:

| pair | finding |
| --- | --- |
| 0003-0032 | Plain reference. RFC-0032 retires `crate_root_extra`, which 0003 never mentions. |
| 0004-0016 | Plain reference, but 0016's banner misreads RFC-0004 §7. Fix 0016. |
| 0012-0064 | A real change: RFC-0064 "finishes it and removes the parts that still contradict it". RFC-0064 is Draft, so link it once it is ruled. |
| 0014-0049 | Plain reference, wrongly attributed: the out-of-tree work was RFC-0013's, now RFC-0087's. Fix 0049. |
| 0017-0047 | Plain reference. |
| 0034-0042 | Plain reference: RFC-0042 says "RFC-0034 D6 stands". |
| 0049-0064 | A proposed change that did NOT land (verified by 133): `nros-platform-config` reads `arch_profiles`, `compile`, `required_env` and `[arch.*]` from platform files only, and board descriptors carry only `[capabilities]` and `[knobs]`. See P7. |
| 0049-0098 | A real change. The frontmatter link is added in this review. |
| 0049-0102 | Plain reference. |
| 0040-0064 | Plain reference. |

## Decision brief (for the user)

**P1. RFC-0003: merge into RFC-0072, or keep it as an amended standalone RFC?**

- What has changed: RFC-0072 restates 0003's principle (nano-ros is a library
  the RTOS imports) and extends it. 0003's deploy contract (sections 4, 5 and
  8) was overtaken by RFC-0098, RFC-0118 and RFC-0120. Its section 9 on NuttX
  `std` is false today: the board crate is `no_std`, and only the make road
  still passes `-Zbuild-std=std,...`, which RFC-0126 Q5 retires.
- Options:
  - (a) Merge the surviving adapter taxonomy into RFC-0072, mark 0003
    Superseded by 0072, and archive it.
  - (b) Keep 0003. Drop section 9 and rewrite section 4 to point at
    RFC-0098, RFC-0118 and RFC-0126.
- Recommendation: (a). Two RFCs stating one principle is the drift this review
  exists to remove.

**P2. RFC-0049's front-ends: amend in place, or supersede that section by
RFC-0103?**

- What has changed: 0049 rejects nros-owned Kconfig and Kconfig generation.
  `zephyr/Kconfig` now carries a block generated from the knob registry
  (RFC-0103 D2b).
- Options:
  - (a) Amend 0049 in place: the knob registry generates the Kconfig
    fragment, and the ladder and the duty rule stand.
  - (b) Mark 0049's "Front-ends" section superseded by RFC-0103 and link the
    two.
- Recommendation: (a). Only the "no generation" rule was reversed; the ladder
  is still the design of record.

**P3. RFC-0034: flip it to Stable after the D6 fix?**

- What has changed: D1 to D8 have landed. The exceptions are D3's single
  bridge (two `platform_aliases.c` remain) and `z_clock`.
- Options:
  - (a) Amend D6, then mark Stable, with D3 noted as open in an issue.
  - (b) Keep it Draft until D3 lands.
- Recommendation: (a), since it is the design of record in every other
  respect. Under the AGENTS.md drift rule, flipping to Stable also updates
  ARCHITECTURE.md.

**P4. Does an implemented Draft become Stable?** (133)

- What has changed: 0053, 0072, 0073, 0076, 0093 and 0114 are Draft, but
  their phases are archived complete. 0072's phase-349 still has parked waves.
- Options:
  - (a) all six become Stable;
  - (b) keep them Draft until each body is re-read against the code;
  - (c) Stable only where the body has no `[OPEN]` and no parked waves:
    0053, 0073, 0076, 0093 and 0114, not 0072.
- Recommendation (133): (c), applied in every area, so that "Draft" means
  something.
- Cross-area: the workspace-system-model brief asks the same question (its
  item 4) and recommends differently. ONE ruling should cover every area. My
  P3 (0034) is an instance of it.

**P5. RFC-0111 has no tracking phase.** (133)

- What has changed: 0111 breaks the 2026-11-12 tracking rule. Its
  `framework_for_board_key` mapping still exists in `nros-orchestration-ir`,
  and 37 tracked files outside `docs/` name the board key it would retire.
- Options:
  - (a) open a phase for its seven steps;
  - (b) archive the RFC;
  - (c) fold it into 0112 D5 as one phase.
- Recommendation (133): (a), with 0112 D5 as one of its items. The RTIC
  dispatch-lint gap is a real defect.

**P6. RFC-0112 after the Tier 0 withdrawal (RFC-0113 D8).** (133)

- What has changed: D1, D2 and D4 are now a record of what was done. D3 and
  D5 are still live, and RFC-0114 asks for its tier numbers in its own words.
- Options:
  - (a) amend in place;
  - (b) supersede with a short RFC that holds D3 and D5;
  - (c) merge D3 and D5 into 0113.
- Recommendation (133): (a), plus verifying RTIC against criterion 5.

**P7. RFC-0064 says LIVE, but its phases are closed.** (133)

- What has changed: its tier and book sections are superseded by 0114, 0115
  and 0124. Steps 6 and 8 never landed.
- Options:
  - (a) close it as Stable and file the open steps as issues;
  - (b) split it into a short Stable RFC (the stance and R5), with the
    ~1,100-line exploration log as an archived companion;
  - (c) keep it LIVE and open a phase for steps 6 and 8.
- Recommendation (133): (b), plus issues.

One stale reference outside the RFCs (133): `examples/README.md` lines 16 and
150 still list `esp32-c3-baremetal` as a platform.

## W3 — applied (2026-11-14)

Rule A1 (a): an RFC becomes Stable only once its open questions and parked
work are filed as issues and the RFC points at them. Applied to P3, P4 and P7.

- **P1 (a).** RFC-0003's surviving adapter taxonomy (the universal rule,
  hook-capable vs hookless, the per-vendor hook table, the 200-line shim
  budget) merged into RFC-0072 §3, and its PX4 multi-component question into
  RFC-0072 §11. RFC-0003 is `Superseded` by 0072 (`superseded-by: 0072`;
  0072 carries `supersedes: [0003]`) and moved to
  `docs/design/archived/0003-rtos-integration-pattern.md` with a forward
  pointer. Every link to its old path repointed (two book pages, two
  integration READMEs, a CLI doc comment, RFC-0034, phase-212, issue 1526);
  phase-542 W9 now records its answer in RFC-0072.
- **P2 (a).** RFC-0049 amended in place: a banner and inline notes record that
  the knob registry generates the Zephyr Kconfig knob section (RFC-0103 D2b),
  reversing only the "no generation" rule. RFC-0103 carries
  `amends: [rfc-0049]`. The 0014-0049 misattribution fixed: out-of-tree
  resolution was RFC-0013's, now RFC-0087's.
- **P3 (a).** RFC-0034's D6 Zephyr row and D7 amended for issue 1324; D3's
  single bridge and `z_clock` filed as issue 2534; status Stable; ARCHITECTURE
  §4 updated.
- **P4 (c).** RFC-0053, 0073, 0076, 0093 and 0114 Stable. 0073's and 0114's
  open questions filed first (issues 2535, 2536); 0093's open word points at
  phase-562 W5; 0053's non-goals are out of scope by decision and need no
  issue. RFC-0072 stays Draft (phase-349 W5/W6 parked).
- **P5 (a).** phase-562 opened (`docs/roadmap/phase-562-image-framework-field.md`,
  owner unassigned): W1-W7 are RFC-0111's seven steps, W8 RFC-0112 D5, W9 the
  RTIC dispatch-lint gap. RFC-0111 and RFC-0112 name it in
  `implements-tracked-by`; 0111 left the rfc-phase-tracking baseline.
- **P6 (a).** RFC-0112 amended in place: D1, D2 and D4 marked as the Tier 0
  record, D3 and D5 live. RTIC checked against criterion 5 by reading the
  code: it fails (fixed task priorities in `main_macro.rs`, no bound asserted
  by its CI cell). Filed as issue 2540.
- **P7 (b).** RFC-0064 split: the Stable RFC keeps the stance, the seam,
  Revision 5 and the open questions; the exploration (worked example, R3's
  matrix, target state, defects, sequencing, the dated log, R4) moved to
  `docs/design/archived/board-support-organization-exploration-log.md`, linked
  both ways. The companion takes no `NNNN-` prefix because `rfc-inventory.py`
  and the id gates read `archived/NNNN-*.md` as an RFC with that number.
  Steps 6 and 8 filed as issues 2537 and 2538, the four open questions as
  issue 2539. 0064 carries `amends: [RFC-0012]` and its banner names the
  three RFCs that superseded its tiers and book rows; RFC-0124 now carries
  `amends: [RFC-0064]` (0114 and 0115 already did). `superseded-in-part-by`
  was not used: W2 dropped it from RFC-0016 as a non-README field.
- **Verdict-table amends.** RFC-0012: dead crates and paths marked, the ABI
  count and the overlay crates noted against RFC-0064 / RFC-0040 D1 /
  RFC-0072. RFC-0016: the banner now reads RFC-0004 right (the `[scheduling]`
  section is retired, the `config.toml` file is not), dead cmake rows marked,
  the app-stack figure points at 128 KiB. RFC-0017: an "As built" section (C
  ABI timer with oneshot and cancel, `AtomicU32`, the refill thunk, a
  caller-wired registration). RFC-0013 moved to `archived/` with pointers to
  RFC-0087 and RFC-0062.
- **`examples/README.md`.** No change needed: it no longer names
  `esp32-c3-baremetal` (checked 2026-11-14).
- `docs/design/README.md` rows and `ARCHITECTURE.md` §2/§4 and "Open design
  fronts" updated for every status change.
