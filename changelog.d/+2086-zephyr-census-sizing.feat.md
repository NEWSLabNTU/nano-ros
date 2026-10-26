A ported ament workspace built as a Zephyr application is now sized from its
census, as on the FreeRTOS and host roads. `entities = "census"` in its
`system.toml` makes the configure take the census before Kconfig. That
census then derives the pools, the executor's callbacks and nodes,
`local_queryable`, and the parameter store with its heap. A program that
creates several nodes is now counted node by node on every road (issue 2086).
