# r5f_hard_float_abi

phase-575 W1's object-level check of the hard-float Cortex-R5F profile, run by
`scripts/check/check-r5f-hard-float-abi.sh`. No board: the C and C++ objects
are built through `cmake/toolchain/arm-freertos-armcr5f.cmake`, the Rust
object for `armv7r-none-eabihf`, and the three are linked together to check
the ABI attribute. The FreeRTOS images this profile builds belong to W2's
out-of-tree fixture board.
