The LAN9118 drivers (lwIP and smoltcp) drop a packet of known length by
reading its words instead of fast-forwarding. QEMU's model leaks FIFO space
on every fast-forward, so under an inbound flood an mps2-an385 / mps3-an536
FreeRTOS image stopped receiving for good once its pbuf pool ran dry
(issue 1948).
