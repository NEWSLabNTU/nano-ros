The pinned QEMU fork now frees the skipped words on a LAN9118 RX
fast-forward, so a guest that drops frames by fast-forward no longer leaks
the RX FIFO until the NIC refuses every frame (issue 1948). The provisioned
QEMU SDK picks this up at its next re-cut.
