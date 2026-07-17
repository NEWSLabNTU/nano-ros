---
id: 220
title: "book/CLI drift round 2: esp32.md still uses the phantom board id; CLI reference missing 4 verbs + 3 subcommands"
status: resolved
type: bug
severity: low
area: docs
related: [issue-0209]
---

## Findings (deep audit 2026-07-17, F3/H3)

- `book/src/getting-started/esp32.md:26` — `nros setup esp32` survives (the
  #209 fix caught installation.md + cli.md but missed this page); the index
  defines only `qemu-esp32-baremetal`.
- `book/src/reference/cli.md` — 4 top-level verbs and 3 subcommands present
  in `cmd/mod.rs` have no reference section (#209 added only `nros init`).

## Fix sketch

Sweep ALL book pages for the phantom id (`rg 'nros setup esp32' book/`);
diff the cli.md section list against the cmd enum and fill the gaps. Add the
"every documented board id exists in nros-sdk-index.toml" assertion to the
audit F3 grep so page-by-page misses stop recurring.

## Resolution (2026-07-17)

All three phantom `nros setup esp32` instances fixed (esp32.md ×2,
integration-esp-idf.md). The "4 missing verbs + 3 subcommands" half was a
FALSE POSITIVE from the haiku docs lane: the only undocumented top-level
verb is `nros release`, which is feature-gated maintainer-only and hidden
from help BY DESIGN; the user-facing verb list matches the book. (Good
calibration data for the deep-audit model table — exactly the
wrong-answer-cheap-to-catch case haiku was budgeted for.)
