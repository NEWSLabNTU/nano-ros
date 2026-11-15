---
id: 2605
title: "starter-zephyr warms the native_sim talker in C and Rust but not C++, which the starter page documents, so a C++ seat builds online"
status: resolved
type: limitation
severity: medium
area: [release, zephyr]
related: [phase-551]
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that adds zephyr:cpp to the default projects"
---

**Home: [phase-551](../../roadmap/phase-551-starter-image.md)** W3.

# The Zephyr starter image does not warm the C++ talker

## Symptom

`book/src/starters/zephyr-native-sim.md` documents the native_sim talker in
three languages (`nros new zs_ws --board zephyr --template talker --lang
rust|c|cpp`). The `starter-zephyr` image built for `0.6.0-nros10` (dispatch run
43687747571, `ghcr.io/newslabntu/nano-ros-starter:0.6.0-nros10-zephyr`) warms,
and rebuilds with `--network=none`, only two of them:

```
ARG NROS_STARTER_ZEPHYR_PROJECTS="zephyr:c zephyr:rust"
```

(`scripts/ci/lib/image-context.sh`). A seat following the page's C++ block
builds what the image did not warm, which needs the network the image exists
to make unnecessary.

## Why it was not simply added

The image is 4.87 GB compressed against RFC-0127 D7's 5.00 GB budget, 2.6 %
headroom, and `scripts/ci/starter-image.sh` fails the build over budget, so
what warming C++ costs had to be measured before adding it.

## Measured (2026-11-15)

`starter-zephyr` built locally from the `nros-0.6.0-nros10` tag (no push) with
`--build-arg NROS_STARTER_ZEPHYR_PROJECTS="zephyr:c zephyr:rust zephyr:cpp"`:
all three talkers warmed, then rebuilt with `--network=none` and
`NROS_OFFLINE=1`, exit 0.

| | raw | compressed (gzip -6) |
| --- | --- | --- |
| published `0.6.0-nros10-zephyr` (C, Rust) | 15.88 GB | 4.87 GB |
| C, Rust, C++ (local) | 15.89 GB (15,890,810,315 B) | 4.87 GB |

C++ adds about 0.01 GB raw and nothing visible at the compressed scale: the
C++ talker reuses the Zephyr SDK, the module sources and the cargo registry the
other two already warmed. It fits the budget.

## Resolution

`zephyr:cpp` joined `NROS_STARTER_ZEPHYR_PROJECTS`'s default in
`scripts/ci/lib/image-context.sh`, so `starter-zephyr` warms, and rebuilds
offline, all three languages the page documents. It takes effect on the next
release's image; `0.6.0-nros10-zephyr` is immutable and stays C and Rust.
