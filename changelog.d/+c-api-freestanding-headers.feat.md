C API: the public headers (`<rcl/*.h>`, `<rclc/*.h>`, `<rcl_action/*.h>`,
`<rcl_lifecycle/*.h>`, `<rclc_lifecycle/*.h>`, `<rclc_parameter/*.h>`,
`<rosidl_runtime_c/*.h>`) are freestanding: they compile with
`-ffreestanding -nostdinc`, so a target with no libc can include them. They no
longer pull in `<stdio.h>` or `<string.h>`; a C file that called `printf` or
`memcpy` without including those headers itself must now include them.
