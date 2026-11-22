A Zephyr image's SNTP re-sync thread no longer runs on a fixed 2 KiB stack,
which Zephyr's SNTP query can overflow silently (7 KiB was measured on a
64-bit Arm image). Its stack is now `[image.<id>.knobs.clock]
sntp_resync_stack_bytes`, 8 KiB by default, and the build's stack-floor walk
checks it. `sntp_resync_interval_ms` is counted on the image's own clock,
which on a simulator is guest time: a simulator running faster than real
time re-queries the server more often in real time.
