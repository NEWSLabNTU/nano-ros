---
id: 2182
title: "The `nros-nsos` snippet refused every mps2/an385 build of the generic `zephyr` board"
status: resolved
resolved_in: "2026-10-31"
type: bug
area: [zephyr, boards]
severity: high
found: 2026-10-31
related: [2157]
---

## What was measured

`just build zephyr` on main after PR #2866 (issue 2157): every
`build-cortex-m-*` row failed at Kconfig, for example
`build-cortex-m-c-talker-zenoh`:

```
-- nano-ros: image `zephyr_zenoh` -> board snippet(s) nros-nsos (the board's `[board.zephyr] snippets`)
-- Snippet(s): nros-nsos
... 'y' but got the value 'n'. Check these unsatisfied dependencies: ARCH_POSIX (=n), NATIVE_LIBRARY
.../zephyr/snippets/nros-nsos/nsos.conf:15: warning: attempt to assign the value 'y' to the undefined symbol NATIVE_SIM_SLOWDOWN_TO_REAL_TIME
error: Aborting due to Kconfig warnings
```

Five rows failed in that run: `build-cortex-m-{c-talker,c-boot-report,cpp-talker,
rust-talker,rust-talker-serial}-zenoh`, every one of them an image of the generic
board built for `mps2/an385`.

## Cause

2157 put `snippets = ["nros-nsos"]` on the generic `zephyr` board and made the
snippet snippet-wide on purpose, reasoning that a non-native board would never
name it, and that a Kconfig refusal is louder than a no-op. The first half was
false: the generic board's `west_board = "native_sim/native/64"` is only a
DEFAULT, and the fixture lane builds the same board with `-b mps2/an385`. The
refusal was measured on mps2 during 2157 and read as the intended behaviour. The
2157 PR rebuilt native_sim rows only, so nothing built an mps2 row.

## Resolution

The snippet is board-scoped: `boards: /native_sim(/.*)?/:` holds the append. Only
the BOARD a build actually targets can say whether it is native, and this regex is
where Zephyr asks it. Zephyr matches it against `${BOARD}${BOARD_QUALIFIERS}` on
3.7 and `${BOARD}/${BOARD_QUALIFIERS}` on 4.4, read from each line's
`scripts/snippets.py`; the regex accepts all three spellings on both.

Gate `check-nsos-single-source` now also refuses a snippet-wide `append:`, and
requires a `native_sim` board entry. It was mutation-tested: restoring the old
`snippet.yml` makes it fail, naming this issue.

Cost: a native simulator board under another name no longer gets NSOS from this
snippet. It needs a snippet of its own.

## Measured

`just build zephyr` (Zephyr 3.7) with the scoped snippet: rc 0, all five rows
above build, west fixtures 5/5. Each row's `.config` under
`build/zephyr-workspace-builds/3.7/` (84 rows):

| Board | Rows | `NET_NATIVE_OFFLOADED_SOCKETS=y` | `NATIVE_SIM_SLOWDOWN_TO_REAL_TIME=y` |
|---|---|---|---|
| native_sim | 72 | 72 | 72 |
| native_sim (`build-logging-smoke`, own `prj.conf`, names no snippet) | 1 | 0 | 1 |
| mps2 | 10 | 0 | 0 |
| qemu_cortex_a53 | 1 | 0 | 0 |
