//! Network poll callback and global state for ESP32-C3 QEMU (OpenEth).

use openeth_smoltcp::OpenEth;

nros_smoltcp::define_network_state!(
    NETWORK_STATE: OpenEth,
    poll = smoltcp_network_poll,
    before_poll = {
        // QEMU's `open_eth` model flushes queued ingress packets when
        // RXEN transitions from 0 to 1. Toggle periodically so slirp
        // packets do not sit queued while the guest busy-polls.
        use core::cell::UnsafeCell;
        struct S(UnsafeCell<u32>);
        unsafe impl Sync for S {}
        static CNT: S = S(UnsafeCell::new(0));
        let c = CNT.0.get();
        *c = c.read().wrapping_add(1);
        if *c % 8 == 0 {
            let moder_addr = 0x600C_D000usize as *mut u32;
            let cur = core::ptr::read_volatile(moder_addr);
            // Drop RXEN, then restore — triggers qemu_flush_queued_packets().
            core::ptr::write_volatile(moder_addr, cur & !0x1);
            core::ptr::write_volatile(moder_addr, cur);
        }
    }
);
