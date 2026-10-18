A board can now state its measured per-frame receive cost, `[board.rx]
frame_cost_us = <us>` (optionally `floor_us`), in `nros-board.toml`. The zenoh
read-task budget derived from a subscriber's declared `ingress` then uses
RFC-0074's full relation, `REST = burst x (10^6/rate_hz - c)`, and the build
refuses a declaration the board cannot satisfy: a `rate_hz` above `10^6 / c`, or
a burst whose `c x burst + floor` exceeds the tightest tier's slack (reported as
not checked, never guessed, when no tier states a deadline or period). The
mps2-an385 FreeRTOS board states RFC-0074's measurement (940 us, 11 ms floor).
A board without `[board.rx]` behaves exactly as before.
