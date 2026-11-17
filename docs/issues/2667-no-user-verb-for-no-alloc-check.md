---
id: 2667
title: "A user has no verb to check that a built image links no allocator: `check-no-alloc-image` is a checkout script"
status: open
type: enhancement
area: [cli, docs]
severity: low
found: 2026-11-17
related: [phase-569, rfc-0133, issue-0816, issue-2646]
---

**Home:** [phase-569](../roadmap/phase-569-discoverability.md) W7 (an image
noun's checks), under [RFC-0133](../design/0133-discoverability.md). It is a
discoverability gap: the check exists, and a user cannot reach it from `nros`.

## What happens

`scripts/check-no-alloc-image.py` (issue 0816) reads a linked image and
reports every allocation symbol by family (`c-heap`, `rust-alloc-shim`,
`cxx-operator-new`, `rtos-heap`) against a tier (`heap-free`, or `unified`
over objects). The project runs it on its own heap-free image in the L3 lane.

A user cannot run it. A release ships no `scripts/`, the book may not tell a
user to run a checkout script (`check-book-no-checkout`), and no `nros` verb
wraps it. So the book's
[Bring your own C++ library](../../book/src/guide/build/bring-your-own-cpp-library.md)
page can warn that a package calling `nano_ros_hosted_cxx()` may link
`operator new`, and can only link the script on GitHub as reading material.
It cannot give a step that checks the user's own image.

## Direction

A user verb on the image noun, e.g. `nros image check <image> --no-alloc`
(the spelling is W7's to settle with RFC-0133's grammar), that:

- runs the same reader over the image `nros build` just made for that image,
  with the tier named (`--tier heap-free` default; `unified` reads the
  build's objects, as the script does);
- prints each symbol with its family and the object that references it, and
  takes `--json` under a `nros.image.check.v1` schema;
- exits non-zero on a finding, so a user's CI can hold an image to it.

One implementation: the verb and the L3 lane call the same reader, so the
script does not become a second spelling.

Acceptance: the own-library page's warning gains a probed step that runs the
verb on the Eigen talker image and shows `operator new` reported (or not, for
the fixed-size talker), and the L3 lane's `--tier heap-free` run goes through
the same code.
