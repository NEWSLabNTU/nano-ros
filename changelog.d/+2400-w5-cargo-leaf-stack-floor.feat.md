`nros build` now checks the main task's stack on FreeRTOS images it builds
with cargo. After cargo links, it walks the image's deepest path and compares
the floor with the stack the board's build resolved
(`NROS_FREERTOS_APP_STACK_KB`).

- An image with a measured allowance fails when its floor is above that stack,
  and names the value to set.
- An unmeasured image warns and still builds.
- Boards that state no stack on this road print "not checked".
- `NROS_STACK_FLOOR=OFF` skips the check.
