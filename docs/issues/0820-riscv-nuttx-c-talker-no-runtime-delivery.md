---
id: 820
title: "`c_riscv_nuttx_e2e` failed on a MUSEUM BINARY — the riscv C talker
  published on a domain its own sources do not produce (issue 0475's symptom,
  after 0475 was resolved)"
status: open
type: bug
area: cmake, testing
related: [issue-0475, issue-0445, issue-0196]
---

## Resolution of the reported failure: stale artifact, not a code defect

`c_riscv_nuttx_talker_delivers_cross_process` **passes in 3.5 s** after
`rm -rf examples/qemu-riscv-nuttx/c/talker/build-zenoh` and a rebuild, on
unmodified sources. It failed at the full 90 s timeout before that.

The binary the tier-2 fixture build left behind published on ROS domain **1**.
The same leaf, rebuilt clean from the same commit, publishes on domain **0** —
which is what the test's listener subscribes to, and what the sources say:

```
[probe] getenv=(null) NROS_ENTRY_DOMAIN_ID=0 domain_id=0
[probe] resolve_session_and_domain single: support=0 session=0 -> 0
```

Same guest, same two listeners, before and after the wipe:

| listener | museum binary | after rm -rf + rebuild |
| --- | --- | --- |
| `ROS_DOMAIN_ID=0` (what the test uses) | 0 received | **24 received** |
| `ROS_DOMAIN_ID=1` | 5 received | 0 received |

The only change between those two columns is the wipe. So the old ARCHIVE code
linked into that image behaved differently from the archive code in the tree.

## Why this stays open

**Issue 0475 is marked resolved** (phase-209: `LINK_DEPENDS` on the consuming
target, so a backend `.a` gains a real rebuild edge instead of an order-only
one). This leaf reproduced 0475's symptom anyway, which means either the fix
does not reach `examples/qemu-riscv-nuttx/c/talker` / the `just nuttx
build-riscv-c` path, or the tier-2 fixture build reached the binary another way.
That is the open question, and it is the one worth answering: a museum binary
here is indistinguishable from a code defect until someone wipes the directory.

Verify with the 0475 recipe: `ninja -C <build-dir> -t query <exe>` — the RMW
`.a` must appear under `|`, not `||`, and touching a backend source must relink.

## The staleness probe cannot see this class, and I asserted otherwise

An earlier revision of this issue said "**Not a stale fixture**" and justified it
by mtime: the talker was rebuilt 2026-08-26 23:54, after the tree's last source
edit. That reasoning is structurally void for this failure mode. A 0475 museum
binary is NEWER than its sources while containing older archive content —
there is no dependency edge, so nothing moves the mtime. **An mtime freshness
check cannot detect the very class it was invoked against**, and reporting its
result as evidence is the same shape as issue 0445's absorbing STALE verdict,
one level down.

Everything that was built on that claim was wrong with it: a "domain mismatch
root cause", a comparison to issue 0801, and a suspected sentinel defect at
`packages/api/nros-c/src/node.rs` (`if support_domain != 0`, where 0 is both a
legal ROS domain and the unset marker). That sentinel ambiguity IS real code and
may deserve its own issue, but it is NOT what broke this test — the native C
talker, same source, resolves to domain 0 with no split, and the instrumented
riscv image now does too.

## Recorded for whoever picks this up

* `domain_of(NuttxRiscv, C, Pubsub)` = **86**, measured. The `1` came from
  neither the allocator nor any cmake define (`build.ninja` bakes only
  `NROS_ENTRY_LOCATOR`), which is consistent with it coming from archive code
  that predates the current resolution.
* Instrumenting this image is harder than it looks and three separate NULL
  results each nearly read as "the branch never runs": `nros_log` output goes
  nowhere (the image installs no sink), a probe string can be absent from the
  ELF (that IS the museum binary — check with `strings <elf> | grep <probe>`),
  and with no router running `nros_support_init` returns `-4` so entity creation
  is never reached at all.

## Symptom

Tier 2 (`just ci-matrix`), 2026-08-27. Of 1704 tests, two failed; one
(`test_qemu_rtic_service_e2e`) passes solo and is the usual in-sweep QEMU flake.
This one does not.

```
nros-tests::c_riscv_nuttx_e2e c_riscv_nuttx_talker_delivers_cross_process
  panicked at packages/testing/nros-tests/tests/c_riscv_nuttx_e2e.rs:94:13:
  native listener never received the riscv-nuttx C talker's /chatter — the
  riscv C-lane runtime delivery did not work (archived 0199 fixed the link;
  this is the runtime half)
```

Reproduced SOLO twice, `--retries 0`, 90.3 s each (the test's own
`wait_for_output_count(..., 3, 90s)` budget). The test already carries
`retries = 1` in `.config/nextest.toml` and failed in-sweep with it.

## ROOT CAUSE: a domain mismatch, proven by experiment (2026-08-27)

The guest publishes on ROS domain **1**; the test's native listener subscribes
on domain **0**. The domain is the FIRST segment of every rmw_zenoh keyexpr, so
the subscription can never match the publication — no error anywhere, which is
why this looked like a transport failure.

One run, one guest, two listeners differing only in `ROS_DOMAIN_ID`:

| listener | subscribes to | result |
| --- | --- | --- |
| `ROS_DOMAIN_ID=0` (what the test does) | `0/chatter/std_msgs::msg::dds_::String_/*` | nothing |
| `ROS_DOMAIN_ID=1` | `1/chatter/...` | **`I heard: [Hello World: 1..5]`** |

The guest published normally throughout (`Publishing: 'Hello World: N'`). So the
image is healthy and the riscv C lane's runtime delivery WORKS — the test asserts
against a listener on the wrong domain.

## The guest splits its own domain mid-session

From the router (`ZENOHD_LOG=debug`), one ZID, in order:

```
Declare token 1  @ros2_lv/0/<ZID>/0/0/NN/%/%/node        <- domain 0
Undeclare token 1
Declare token 4  @ros2_lv/1/<ZID>/0/0/NN/%/%/talker      <- domain 1
Declare token 5  @ros2_lv/1/<ZID>/0/3/MP/%/%/talker/%chatter/...
Declare interest   1/chatter/std_msgs::msg::dds_::String_/...
```

The default node created by `nros_support_init` lands on 0; the node created by
`nros_node_init` and its publisher land on 1. That is a single session using two
domains — **the exact shape of issue 0801**, which is marked resolved. 0801 was
the mirror image (node token on the configured domain, entities on 0); this is
the same defect with the operands swapped, so 0801's fix traded one direction
for the other rather than removing the ambiguity.

The suspect mechanism, from `packages/api/nros-c/src/node.rs`:

```rust
let support_domain = support_mut.domain_id as u32;
let domain_id = if support_domain != 0 { support_domain } else { session.domain_id() };
```

`0` is a legitimate ROS domain AND the "unset" sentinel, so a caller that
resolves to domain 0 has its answer discarded. The C ABI already has a distinct
spelling for this (`DOMAIN_ID_EXPLICIT_ZERO_C_ABI` = 255 →
`baked_domain_from_c_abi` → `Some(0)`), and the example does not use it: it
passes plain `0`, because `nros/app_main.h` defaults `NROS_ENTRY_DOMAIN_ID` to
`0` and the fixture row bakes no domain at all.

## What is NOT established

* **Where the value `1` comes from.** Not the allocator (`domain_of(NuttxRiscv,
  C, Pubsub)` = **86**, measured). Not a cmake define (`build.ninja` bakes only
  `NROS_ENTRY_LOCATOR`). `support.domain_id` resolves to 0 and
  `open_session(..., support.domain_id, ...)` opens the session with that same 0,
  so `session.domain_id()` should also be 0. Reading the code predicts 0 at every
  step and the wire says 1, three times over — so one of those steps does not do
  what it reads like, and the next move is to instrument rather than re-read.
* **Whether the fix belongs in the fixture row, `app_main.h`, or node.rs.** All
  three are implicated; picking before the `1` is explained would be guessing.
  Note the sibling riscv Rust rows DO set `NROS_DOMAIN_ID = "0"` explicitly and
  this C row sets nothing, which is suggestive but does not by itself produce a 1.
* **Whether it fails on origin/main.** Unchanged from below — still not run.

## Earlier findings (still true)

* **Not a stale fixture.** The riscv C talker was rebuilt 2026-08-26 23:54 by the
  `lane=tier2` build, after the tree's last source edit.
* **The listener side starts.** The panic is at `wait_for_output_count`, not the
  readiness wait.
* **Not an in-sweep flake.** Fails solo on an idle host; its sweep sibling
  `test_qemu_rtic_service_e2e` passes solo.
* **phase-384 W1 is not implicated.** Its six edits are all on RECEIVE paths; the
  talker is a C publisher. The domain evidence above now makes this moot.

## Reproduce

```
source ./activate.sh
just build-test-fixtures lane=tier2      # or: just nuttx build-riscv-c
cargo nextest run -p nros-tests --test c_riscv_nuttx_e2e --retries 0 \
    -E 'test(c_riscv_nuttx_talker_delivers_cross_process)'
```

Needs `zenohd` and `qemu-system-riscv32`; the test skips cleanly without them.
