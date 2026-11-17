# `talker`: fixture-r5f-freertos / c

One of the board package's examples (RFC-0135 D7), one per language: the ROS 2 tutorial talker, as a
C component, built for the out-of-tree board `fixture-r5f-freertos`.

```bash
export NROS_EXTRA_BOARD_PATH=<the directory holding nros-board-fixture-r5f-freertos>
export FIXTURE_R5F_SDK_ROOT=<the vendor SDK root>
nros build fixture-r5f-freertos
```

The board's network driver is outside this fixture, so the image builds and
does not publish.
