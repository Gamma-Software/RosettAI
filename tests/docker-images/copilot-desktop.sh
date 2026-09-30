#!/bin/sh

set -eu

display_number=99
export DISPLAY=":$display_number"

Xvfb "$DISPLAY" -screen 0 1440x900x24 -nolisten tcp &
display_pid=$!
trap 'kill "$display_pid" "${vnc_pid:-}" 2>/dev/null || true' EXIT

sleep 1
x11vnc -display "$DISPLAY" -forever -shared -nopw -rfbport 5900 -quiet &
vnc_pid=$!

printf '%s\n' 'VS Code is available over VNC on port 5900.'
code --wait --no-sandbox --disable-gpu --disable-dev-shm-usage /workspace
