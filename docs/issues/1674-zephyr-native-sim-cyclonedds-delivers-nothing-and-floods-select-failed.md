---
id: 1674
title: "Zephyr native_sim + Cyclone DDS delivers nothing again — every e2e cell,
  C and C++, `received 0 sample(s)` — and each image floods
  `os_sockWaitsetWait: select failed` fast enough to OOM the test runner"
status: open
type: bug
area: [zephyr, rmw, testing]
severity: high
found: 2026-10-03
related: [0155, 0968, 1673]
---

## Symptom

Every Cyclone DDS e2e cell on Zephyr `native_sim` fails, in both languages, on
every retry:

| cell | result |
| --- | --- |
| `example_e2e::case_11_cyclonedds_c_pubsub_e2e` | FAIL 3/3, ~23 s |
| `example_e2e::case_12_cyclonedds_cpp_pubsub_e2e` | FAIL 3/3, ~23 s |
| `example_e2e::case_14_cyclonedds_c_service_e2e` | FAIL 3/3, ~28 s |
| `example_e2e::case_15_cyclonedds_cpp_service_e2e` | FAIL 3/3, ~28 s |
| `example_e2e::case_17_cyclonedds_c_action_e2e` | FAIL, ~80 s |
| `example_e2e::case_18_cyclonedds_cpp_action_e2e` | FAIL, ~80 s |

```
[cyclonedds/cpp/Pubsub] listener received 0 sample(s), expected ≥1
```

The eight `boot_smoke` cells for the same images PASS — they boot. Then they
deliver nothing.

Each image's console repeats one line:

```
[00:00:00.000,002] <inf> cyclonedds: cyclone: os_sockWaitsetWait: select failed, retcode = -
```

with the clock frozen at `00:00:00.000,002`, i.e. a busy loop that never
yields time.

## It is NOT issue 1673's change — measured with an A/B

Found while verifying 1673, which changed these images' build. To separate the
two, the C++ talker and listener were rebuilt from `origin/main`'s own
`CMakeLists.txt` (1673's generator change is a no-op whenever an example's hand
descriptor block is present, so this reproduces `main` exactly) and the one cell
re-run:

```
example_e2e::case_12_cyclonedds_cpp_pubsub_e2e   FAIL [22.622s]
[cyclonedds/cpp/Pubsub] listener received 0 sample(s), expected ≥1
```

Identical. The failure predates 1673 and does not depend on it. The C images
could not be checked the same way: on `main` they do not LINK (that is 1673).

## It has happened before

Issue **0155** — "Zephyr+CycloneDDS lane: images boot, net ready, then silence —
no publish, no error" — is this symptom, and is resolved. So this is a
regression of a fixed defect, not a new one. It may also be among the ~12
unreproduced tier-2 runtime failures issue **0968** counts; this is a
reproduction of at least six.

## The flood is a hazard of its own

One `nextest` run of these six cells produced **566,291,852** copies of that
line — a **55 GB** captured log — and `cargo-nextest`'s in-memory capture grew
to **91 GB** resident before the kernel OOM-killed it:

```
Out of memory: Killed process … (cargo-nextest) … anon-rss:91374324kB
```

So a single failing Cyclone native_sim cell can take down the machine it runs
on, and every other session sharing it. Independently of the delivery fix:

* the Zephyr Cyclone log line inside that loop wants rate-limiting at the
  source, or the loop wants to yield;
* the e2e harness should bound what it captures from a guest console.

A bounded reproduction that is safe to run, used for the A/B above:

```sh
timeout 300 cargo nextest run -p nros-tests --cargo-profile nros-relwithdebinfo \
  -E 'binary(zephyr) and test(=example_e2e::case_12_cyclonedds_cpp_pubsub_e2e)' \
  --no-capture --retries 0 2>&1 | grep -v "select failed"
```

`--no-capture` streams instead of buffering, and the filter keeps the flood off
the disk.

## Not established

The cause. `select` failing with a frozen clock on native_sim points at the
NSOS socket-offload path the test's own message names ("16 MiB malloc arena +
NSOS offload overlay parity"), and at whatever resolved 0155 — but neither was
measured here. When the regression landed is also not established; `lane=all`
had not reached these cells on this host before.
