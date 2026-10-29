A ThreadX image (the Linux simulator and the riscv64 QEMU board) now waits
between its zenoh session-open attempts, so it connects to a router started a
moment after it instead of failing with `ConnectionFailed` 0.13 s in. The
bounded retry (ten attempts, 300 ms apart) was already shared by every board,
but on ThreadX its backoff called `zpico_spin_once` on the not-yet-open
session, which returns at once, so all ten attempts ran back to back. The
backoff is now the platform sleep on every platform.
