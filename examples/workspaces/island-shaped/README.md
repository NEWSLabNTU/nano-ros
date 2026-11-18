# Island-shaped workspace

One project shaped like the safety island, a downstream nano-ros consumer.
It exists so a layout the island hit breaks the nano-ros merge
queue instead of the island's next build. It is not a tutorial.

```text
island-shaped/
├── .colcon_workspace
├── nros-codegen.toml            # std_msgs/String bound (two subscribed types)
└── src/
    ├── c_controller_pkg/        # C component; its param files live in config/
    ├── rust_monitor_pkg/        # Rust component, in the same image
    └── island_bringup/          # system.toml, launch/
```

| layout | where | what broke on it |
| --- | --- | --- |
| `<param from>` a file in a sibling package, absolute (`$(find-pkg-share ...)`) and `$(dirname)`-relative | `island_bringup/launch/system.launch.xml` | every model consumer refused an absolute input |
| a C and a Rust component in one image, no C++ | `[[component]]` rows | the generated entry used the C entry pack, which cannot construct a Rust component |
| pools stated above the derived counts | `[image.freertos]` and `[image.zephyr]` knobs | the knob delivery check refused a stated Kconfig value after link |
| a census and a board build | `[image.linux]`, `[image.freertos]` | |

Where it is checked:

- merge queue (`check-workspace-all`): `just check island-shaped-workspace`
  takes the census, builds `island_bringup:freertos`, and runs
  `check-knob-delivery` over the committed snapshot of the Zephyr image's
  configured build dir (`tests/fixtures/knob-delivery/island-shaped-zephyr`).
- daily (nightly.yml): `just check island-shaped-zephyr` builds
  `island_bringup:zephyr` on a west workspace, checks its live build dir, and
  fails if the snapshot differs from it. `just check island-shaped-zephyr-regen`
  rewrites the snapshot.

When a downstream project reports a layout the tree did not cover, add it here
in the same PR as its fix.
