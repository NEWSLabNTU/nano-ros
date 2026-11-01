New board package `rpi-pico-w-zephyr` (Raspberry Pi Pico W on Zephyr 3.7,
serial only), with Wokwi as its emulator twin: `nros setup rpi-pico-w-zephyr`
installs the Zephyr SDK and `wokwi-cli`. A twin run also needs a
`WOKWI_CLI_TOKEN`. The board's Kconfig turns off the tickless kernel, which
Wokwi's SysTick does not carry, so the twin and the bench boot the same image.
