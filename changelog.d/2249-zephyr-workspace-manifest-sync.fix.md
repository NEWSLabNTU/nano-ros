`just zephyr setup` now brings an existing west workspace to the checkout's
manifest. When the workspace's copy of `west.yml` differs from the checkout's,
setup refreshes it and runs `west update`; when they match, setup touches no
network. Before this, a module added to the manifest (phase-524's `hal_stm32`)
never reached a workspace that already existed (issue 2249).
