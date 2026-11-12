`just zephyr stack-floor-twin` re-measures each stack-floor allowance on the
board's Renode twin and fails when a row no longer matches what the image
does. Every build now writes what it measured to
`nros/stack-floor/report.json`. The first run found the nucleo C talker's
row 180 B low, and its floor rises from 12,872 to 13,064 B.
