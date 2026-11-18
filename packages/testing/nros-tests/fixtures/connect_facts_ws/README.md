# connect_facts_ws

Test fixture for issue 2714: on the `linux` board, a domain and locator the
image's `system.toml` states are the baked rung, below `$ROS_DOMAIN_ID` /
`$NROS_LOCATOR`, and the environment overriding a stated value warns at boot.
One bringup per language (C, C++, Rust), each with a `<lang>_stated` image (its own domain and a
locator) and a `<lang>_unstated` one (`[system]`'s required `domain_id = 0`, no
locator). Consumed by `packages/testing/nros-tests/tests/host_connect_facts.rs`.
