The qemu_cortex_a53 SMP realtime image states a 10 s zenoh connect budget, so a frame QEMU's user network drops during the open handshake no longer ends the boot with `ConnectionFailed`.
