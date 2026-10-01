---
id: 1526
title: "Three RFCs still describe the ESP-IDF port as shipped, naming files deleted in phase-468 W2"
status: resolved
area: docs, esp32
severity: medium
phases: [468]
rfcs: [0003, 0012, 0014, 0064, 0072]
resolved: 2026-10-01
related: [1525, 1211]
---

# Three RFCs still describe the ESP-IDF port as shipped

phase-468 W2 (2026-09-25, `083d2c10d`) deleted `packages/platform/nros-platform-esp-idf/`,
`integrations/nano-ros/` (the IDF component shell), `cmake/platform/nano-ros-esp_idf.cmake`
and the `esp_idf` platform alternative. The book was updated in the same wave —
`book/src/getting-started/integration-esp-idf.md` is a tombstone and
`supported-boards.md` says "Retired". **The design docs were not**, and they are
the layer CLAUDE.md sends a reader to for "a specific design decision".

Found while writing the RFC-0065 D3 amendment that deleted the last surviving
piece, `Driver::IdfPy` (issue 1525).

## The sites

| file | line | what it claims |
| --- | --- | --- |
| `docs/design/0003-rtos-integration-pattern.md` | 45 | diagram box `integrations/nano-ros/ — ESP-IDF component` |
| | 73 | table row: `ESP-IDF │ idf.py │ integrations/nano-ros/ ESP-IDF component (idf_component_register + Kconfig.projbuild)` |
| | 112–114 | a three-step user workflow ending `idf.py -B build/<b> flash monitor` |
| | 145 | `ESP-IDF │ ~150 (CMakeLists.txt + Kconfig.projbuild + idf_component.yml) │ comfortable │ **already shipped**` |
| | 157 | ESP-IDF listed among "hook-capable vendors" |
| `docs/design/0072-rtos-integration-nano-ros-is-a-guest.md` | 34 | `ESP-IDF │ integrations/nano-ros/ │ CMakeLists.txt + idf_component.yml + Kconfig.projbuild` |
| | 88 | ESP-IDF listed as hook-capable |
| | 569 | a whole worked section `### 6.8 ESP32 — IDF component`, with `[board_config."esp32-qemu"] idf.dir = "{env:IDF_PATH}"` |
| | 935 | `ESP-IDF │ idf.py add-dependency nano-ros` |
| `docs/design/0014-nros-setup-toolchain-management.md` | 194 | board map `esp32-c3 → { esp-toolchain, esp-idf|baremetal, rmw }` |
| | 400 | disk-budget row `esp32 │ 1.4 GB │ ESP-IDF source tree` |

`docs/design/ARCHITECTURE.md:41` had the same defect (`plus an esp-idf platform
integration for ESP32 targets`) and is fixed in the PR that files this issue,
because it is one line. These are not: 0003 and 0072 each carry a worked
section, and 0014's `[sdk.*]` claim is about a provisioning model that was never
implemented — `nros-sdk-index.toml` has no ESP-IDF entry at all.

## Why this is worth an issue rather than a sweep

`idf_component_register` is cited in three places as **prior art** (RFC-0065
D12, its Related-work section, `cmake/NanoRosSupportLibrary.cmake:65`) and those
citations are correct and should stay. So this is not `grep -l ESP-IDF && sed`;
each site has to be read for whether it says "we ship this" or "they do it this
way, and they are right". The same distinction decided issue 1525: *passive
accommodation stays, active claim goes.*

This is the shape issue 1211 measured for the package-directory list — an
authoritative-looking document naming directories that do not exist. 0003's
`**already shipped**` is the worst of the eleven: a reader costing out an ESP-IDF
integration would read it as a solved problem.

## Acceptance

- [ ] RFC-0003 §"ESP-IDF" rows, workflow and cost table state the retirement and
      name phase-468 W2, or move to a "what we tried" section.
- [ ] RFC-0072 §6.8 likewise; its `:108` already records the real board
      (`esp32-qemu │ esp32 │ none │ board-run`), so the two halves of that
      document currently disagree with each other.
- [ ] RFC-0014's board map and disk budget drop the ESP-IDF rows, or mark them
      as the unimplemented model they are (see issue 1525's acceptance, which
      would restore the `[tool.esp-idf]` entry those rows presuppose).
- [ ] The `idf_component_register` prior-art citations are left alone, and the
      commit message says which sites were read and kept.

## Resolution (2026-10-01)

### The census was short by two RFCs

This issue named three (0003, 0014, 0072) and listed eleven sites. Deriving the
population instead of working the table — `git grep` for the DELETED artifacts
rather than for the words "ESP-IDF" — found two more documents making the same
live claim:

| RFC | sites | what it claimed |
| --- | --- | --- |
| **0012** board/BSP integration | 4 | the Layer-3 diagram box, "Phase 139.2 + 139.3 land `integrations/nano-ros/`", the ESP-IDF integration-shape bullet, and an adoption-table row telling a user to run `idf.py add-dependency nano-ros` |
| **0064** board support organization | 3 | *"`integrations/nano-ros/CMakeLists.txt` is the whole ESP-IDF story — 146 lines"* as present-tense evidence, the `cmake/platform/nano-ros-esp_idf.cmake` quote, and *"Layer 3 is real, not aspirational"* leading with the deleted shell |

0064's was the same shape as 0003's `**already shipped**`: a document arguing a
stance, citing a directory that is gone as the evidence the stance works.

### What was read and KEPT

The distinction this issue set — *passive accommodation stays, active claim
goes* — decided every site. Kept untouched:

* every `idf_component_register` prior-art citation (RFC-0012 §§ on vendor hook
  mechanisms, RFC-0024, RFC-0065 D12 and its Related-work table,
  `cmake/NanoRosSupportLibrary.cmake:66`) — all in files this change does not
  touch;
* RFC-0003:14 and RFC-0072:15/53/75/159 — the vendor survey and the hypothesis
  it tested, which are about what Espressif does;
* RFC-0072's `ESP_NETIF_USES_TCPIP_WITH_BSD_API` capability pattern and the
  "a vendor build's own config may be a knob source" rule, both of which are
  prior art this project still follows;
* every `docs/roadmap/archived/*`, `docs/release/*` and audit-findings mention —
  history, correct as written;
* the book, already a tombstone plus a "Retired" row since phase-468 W2.

### What changed

Retirement stated at each site, naming phase-468 W2 (`083d2c10d`), with the
reason the text is kept rather than deleted where it carries something still
true. RFC-0072 §6.8 is the largest: its board row (`esp32-qemu │ esp32 │ none │
board-run`) IS still live and already appeared in §4, so the two halves of that
document disagreed with each other — §6.8 now says which half is current and
keeps the vendor observation it was written for. RFC-0014's two rows are marked
as the unimplemented model they always were: `nros-sdk-index.toml` has never had
an ESP-IDF entry for the 1.4 GB disk budget or the `esp-idf|baremetal`
alternative to describe.

### Acceptance

- [x] RFC-0003's rows, workflow and cost table state the retirement
- [x] RFC-0072 §6.8 likewise, and its two halves now agree
- [x] RFC-0014's board map and disk budget marked as unimplemented
- [x] prior-art citations left alone — verified by `git grep idf_component_register`
      landing only in files this change does not touch
- [x] **plus RFC-0012 and RFC-0064**, which the census did not name
