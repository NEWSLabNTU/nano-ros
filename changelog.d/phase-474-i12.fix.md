Zephyr serial link: a send whose frame fits the TX ring is now a copy and a
return instead of waiting for the wire. `CONFIG_NROS_ZENOH_SERIAL_TX_RING_BYTES`
defaults to 2048 (was 256), which holds a maximum-size frame (1516 bytes)
whole; a send that does not fit still waits, bounded, and is never dropped.
`_z_zephyr_serial_tx_stats.frames_waited` counts the sends that did not fit.
The serial CRC is table-driven (same value on the wire), the serializer
COBS-encodes without a temporary frame, and the Zephyr port's framing buffers
are static instead of two heap allocations per frame (zenoh-pico 739b1cc4,
phase-474 I12).
