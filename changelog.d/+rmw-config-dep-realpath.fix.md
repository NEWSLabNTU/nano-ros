A bringup named through a `../` path (for example
`${CMAKE_CURRENT_SOURCE_DIR}/../my_bringup`) configures again. Since the
bringup's RMW file moved out of `system.toml`, the RMW-file lookup registered
`system.toml` as a configure dependency under that spelling while the entry
registered its real path, and ninja 1.13 refused the build with
"system.toml is defined as an output multiple times". Both paths are now
registered canonically and once.
