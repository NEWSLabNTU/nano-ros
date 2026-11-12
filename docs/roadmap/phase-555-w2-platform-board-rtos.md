# Phase 555 W2 -- platform-board-rtos: verdicts and decision brief

**Status (2026-11-13). OPEN: the 118 half is reviewed; the 133 half (0053,
0064, 0072, 0073, 0076, 0093, 0111, 0112, 0114, 0115) is in progress. The brief
below waits for the user's ruling.**

The platform-board-rtos slice of
[phase-555](phase-555-design-doc-review.md) W2. Reviewers: 118 (0003, 0012,
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
| 0118 | Target execution model | keep. Reviewed by 139 AUO book in the docs-ux-diagnostics slice, which moved its area here (#3657) | area moved |

Tier 2 pairs, read in the newer RFC's own words:

| pair | finding |
| --- | --- |
| 0003-0032 | Plain reference. RFC-0032 retires `crate_root_extra`, which 0003 never mentions. |
| 0004-0016 | Plain reference, but 0016's banner misreads RFC-0004 §7. Fix 0016. |
| 0012-0064 | A real change: RFC-0064 "finishes it and removes the parts that still contradict it". RFC-0064 is Draft, so link it once it is ruled. |
| 0014-0049 | Plain reference, wrongly attributed: the out-of-tree work was RFC-0013's, now RFC-0087's. Fix 0049. |
| 0017-0047 | Plain reference. |
| 0034-0042 | Plain reference: RFC-0042 says "RFC-0034 D6 stands". |
| 0049-0064 | A proposed change: RFC-0064 item 8 extends the ladder to the build block. RFC-0064 is Draft. Whether it landed is unverified. |
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

133's items for the other half are added here when that half is reviewed.
