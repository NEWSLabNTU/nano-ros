---
id: 1020
title: "The C++ parity lane measures the NATIVE API against rclcpp and cannot see
  `rclcpp_compat.hpp` — 589 lines whose entire purpose is the thing being measured"
status: open
type: bug
area: docs, api
related: [phase-379, issue-0818, issue-0254]
---

## Problem

`ours_cpp` (`scripts/api-parity.py:161`) reads three translation units:

```python
CPP_TRANSLATION_UNITS = (
    ("base",      '#include "nros/nros.hpp"\n',           ()),
    ("component", '#include "nros/component_node.hpp"\n', ()),
    ("std",       '#include "nros/nros.hpp"\n',           ("-DNROS_CPP_STD=1",)),
)
```

and passes `{"nros"}` as the namespace filter (`:180`).

`packages/api/nros-cpp/include/nros/rclcpp_compat.hpp` is not among them, and
everything it declares is in namespace `rclcpp`. So it is excluded **twice**:
the header is never included, and its namespace would be filtered out if it
were.

That header is 589 lines whose entire purpose is drop-in compatibility, and it
is what ported code actually reaches — `#include <rclcpp/rclcpp.hpp>` resolves
to it through `cmake/compat/NrosRclcppCompat.cmake`.

**So the 414 `declined` C++ rows measure how far the NATIVE `nros::` API is
from rclcpp, not how far nano-ros is.** For a campaign whose stated goal is
drop-in replacement, that is the wrong denominator, and it is the headline
number.

## This is issue 0818 one level up, and the code says so

The comment directly above the loop (`:162-172`) explains that `component_node.hpp`
and the `NROS_CPP_STD` flavour were added as extra TUs precisely because a
narrower surface "reported as agreement" is 0818's failure. The same argument
applies to the compat shim and was not made.

## Second-order effect: the cross-language comparison is skewed

Decline RATE against each upstream's uncovered surface:

| lane | theirs-only | declined | rate |
| --- | ---: | ---: | ---: |
| C | 373 | 198 | 53% |
| C++ | 717 | 442 | **62%** |
| Rust | 509 | 285 | 56% |

The raw counts (442 vs 198) read as "C++ is twice as choosy". The rates are
within nine points, and most of the remaining gap is that rclcpp's uncovered
surface is 1.9× rclc+rcl's, plus a per-class multiplier: rcl declines a
wait-set concept once as a free function, rclcpp declines it on ten types
(C: 20 wait-set rows, C++: 71).

Any conclusion drawn from the raw C-vs-C++ counts is wrong, and one was drawn
in this campaign before the rates were computed.

## Fix

Add a fourth translation unit for `rclcpp_compat.hpp` and admit namespace
`rclcpp` for it. That needs three decisions first, which is why this is an
issue and not a patch:

1. **Do compat-shim items correlate as ours?** A `rclcpp::Publisher<M>` alias
   that resolves to `nros::Publisher<M>` should correlate `same` against
   upstream's `rclcpp::Publisher<M>` on NAME — but the shape underneath is
   ours. The row is honest only if it says which.
2. **Two surfaces, or one?** A reader wants both "how close is the native API"
   and "what does a ported file hit". Those are different questions and the
   current lane answers only the first. A `surface:` field, or a separate lane,
   rather than silently merging them.
3. **The shim is std-only.** It is reachable only under `NROS_CPP_STD`, so its
   rows carry the same `std_only` marking the third TU already sets (`:189`).

## Known defects the blind spot has been hiding

Found by reading the shim directly, none of them ledgered:

* `rclcpp::Publisher<T>::SharedPtr` — the most common declaration idiom in
  rclcpp source — does not compile. `detail::SharedPtrTrait` (`:96`) was
  written for exactly this and is **dead code**: it appears once in all of
  `packages/api/`, its own definition. None of `publisher/subscription/client/
  service/timer.hpp` declares a nested `SharedPtr`.
* `RCLCPP_INFO_STREAM` and siblings discard their message (issue 1019).
* `rclcpp::init(argc, argv)` drops `--ros-args` silently (`:238`), turning a
  remap into a wrong-topic bug at runtime.
* The shim `Node` has no parameter methods, no `now()`/`get_clock()`, and no
  `create_service`/`create_client`, though every underlying capability exists.

`examples/templates/cpp-port-minimal-publisher/` is the in-tree measurement of
all this and its README claims the source is vendored "verbatim". It is not —
three lines differ, and each is one of the defects above.
