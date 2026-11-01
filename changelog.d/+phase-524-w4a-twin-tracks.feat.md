Two new probe tracks, `just probe renode-nucleo` and `just probe wokwi-pico`,
boot the Nucleo-F767ZI and Pico W serial talkers in their emulator twins
(Renode and Wokwi) from a pristine `ros:humble` container and assert that
`ros2 topic echo` hears them. Both run in the weekly `probe-platforms`
workflow; the Wokwi track needs a `WOKWI_CLI_TOKEN` secret and fails naming it
without one. Board descriptors gain `[board.run.renode]` and
`[board.run.wokwi]`, the twin's serial wiring.
