`nros run` now boots the Nucleo-F767ZI in its Renode twin. A new `renode`
runner reads the board's wiring, puts USART3 (the zenoh link) on a host
terminal device for the router, and streams the board's console. Before
this, `nros run` handed the Nucleo to Zephyr's west build, which has no
Renode target for that board, and refused. The book has a Nucleo-F767ZI
chapter: setup, scaffold, `nros run` in the twin, and `ros2 topic echo`.
