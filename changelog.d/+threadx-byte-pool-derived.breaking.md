The ThreadX byte pool is no longer a fixed 4 MiB: it is derived from the app
thread's stack plus the RMW's measured working set (about 0.8 MiB for a zenoh
image and 1.6 MiB for a Cyclone one on the RISC-V board). An image's sched
tasks are not in that default: an image that declares them states its pool with
`[image.<id>.knobs.memory] heap_bytes` (or `NROS_THREADX_BYTE_POOL_KB`), and
one that does not stops at boot with a message naming the knob and the bytes it
was short.
