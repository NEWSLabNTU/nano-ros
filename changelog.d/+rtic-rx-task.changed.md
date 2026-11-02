RTIC images on mps2-an385 no longer run a 1 ms timer tick to bound each `wfi`.
The generated RTIC app binds a hardware task to the LAN9118 `ETHERNET`
interrupt, which wakes the executor, and the run loop parks until the next
deadline. An idle RTIC listener now takes no timer interrupt at all while the
router's keep-alives arrive.
