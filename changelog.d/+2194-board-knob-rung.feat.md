A board descriptor's top-level `[knobs.<group>]` now takes every knob in the
registry, spelled as an image's `[image.<id>.knobs.<group>]` is: for example
`[knobs.zenoh] max_subscribers = 1` or `[knobs.zephyr] heap_size = 32768`.
Every build road carries the board's value below the image's own, so an image
that states the same knob still wins. On Zephyr the board's knobs become
`default`s in the generated `Kconfig.from-system-toml`. The
`nucleo-f767zi-zephyr` and `rpi-pico-w-zephyr` boards state their RAM budget
this way, and their images no longer restate it.
