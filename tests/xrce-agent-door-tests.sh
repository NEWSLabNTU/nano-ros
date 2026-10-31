#!/usr/bin/env bash
# phase-519 W4 — the XRCE agent door's locator translation, as a table.
#
# `nros_xrce_agent_args` (scripts/dev/xrce-agent.sh) is the only logic between
# a `system.toml` locator and the agent's argv, and a wrong row starts an agent
# on a port nothing dials. Each row is: locator, the NROS_XRCE_AGENT_DEV to set
# (or -), and the argv expected (or REFUSED). No agent is started.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
# shellcheck source=scripts/dev/xrce-agent.sh
source scripts/dev/xrce-agent.sh

fails=0
check() {
    local locator="$1" dev="$2" want="$3" got rc=0
    if [ "$dev" = - ]; then
        got="$(unset NROS_XRCE_AGENT_DEV; nros_xrce_agent_args "$locator" 2>/dev/null | tr '\n' ' ')" || rc=$?
    else
        got="$(NROS_XRCE_AGENT_DEV="$dev" nros_xrce_agent_args "$locator" 2>/dev/null | tr '\n' ' ')" || rc=$?
    fi
    got="${got% }"
    if [ "$want" = REFUSED ]; then
        if [ "$rc" -eq 0 ]; then
            echo "[FAIL] $locator: expected a refusal, got \`$got\`"
            fails=$((fails + 1))
        fi
    elif [ "$rc" -ne 0 ] || [ "$got" != "$want" ]; then
        echo "[FAIL] $locator (dev $dev): expected \`$want\`, got \`$got\` (rc=$rc)"
        fails=$((fails + 1))
    fi
}

check 'udp/127.0.0.1:8888'                    -  'udp4 -p 8888'
check 'udp/0.0.0.0:2018'                      -  'udp4 -p 2018'
check 'tcp/10.0.0.2:2019'                     -  'tcp4 -p 2019'
check 'udp/[::1]:9000'                        -  'udp6 -p 9000'
check 'tcp/[fe80::1]:7'                       -  'tcp6 -p 7'
check '127.0.0.1:8888'                        -  'udp4 -p 8888'
check 'serial//dev/ttyACM0#baudrate=921600'   -  'serial -D /dev/ttyACM0 -b 921600'
check 'serial//dev/ttyUSB1'                   -  'serial -D /dev/ttyUSB1 -b 115200'
check 'serial/UART_0#baudrate=115200'   /dev/pts/7 'serial -D /dev/pts/7 -b 115200'
check 'serial/UART_0#baudrate=115200'         -  REFUSED
check 'serial//dev/ttyACM0#baudrate=fast'     -  REFUSED
check 'custom://ivc0'                         -  REFUSED
check 'udp/127.0.0.1'                         -  REFUSED
check 'udp/127.0.0.1:port'                    -  REFUSED
check 'udp/:8888'                             -  REFUSED
check 'udp/::1:9000'                          -  REFUSED

if [ "$fails" -ne 0 ]; then
    echo "xrce-agent-door: $fails row(s) failed"
    exit 1
fi
echo "xrce-agent-door: OK (16 locator rows)"
