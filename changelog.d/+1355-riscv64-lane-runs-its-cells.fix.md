`just test threadx_riscv64` now runs the platform's `rtos_e2e` pubsub and
service cells, not just its own side tests. The nightly job had gone green
without ever running those cells. A new fast gate, `check platform-lane-cells`,
requires every RTOS platform's `test` recipe to select its own `rtos_e2e`
cells.
