Declaring a parameter no longer copies its value through every frame of the
declare path. At the default capacities a `ParameterValue` is about 8 KiB, and
one `declare_parameter<double>` used to take about 67 KiB of task stack on
Cortex-M3, enough to overflow the C++ FreeRTOS carrier's 64 KiB app stack into
the kernel's globals. The value is now borrowed down to its table slot and
copied once. Registering the lifecycle services no longer builds their ten
receive buffers on the stack before boxing them (a 92 KiB frame for Nav2's
`velocity_smoother`). Both phase-518 nodes now boot on the default 64 KiB app
stack at the default capacities (issue 2070).
