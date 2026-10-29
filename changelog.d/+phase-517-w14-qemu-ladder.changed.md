The QEMU command `nros build` prints for a NuttX arm `virt`, NuttX rv-virt or
ThreadX riscv64 image now comes from the same code the test harness launches
(the new `nros-qemu-ladder` crate), so the two can no longer drift. The printed
line now puts `-kernel <image>` before the NIC arguments, and the ThreadX
riscv64 line names the MAC (`52:54:00:12:34:56`) that the firmware expects.
