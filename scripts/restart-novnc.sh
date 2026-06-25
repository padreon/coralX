#!/usr/bin/env bash
# Restart Xtigervnc + noVNC proxy (port 6080).
# Usage: bash scripts/restart-novnc.sh

set -e

echo "[1/4] Killing old processes..."
sudo fuser -k 6080/tcp 2>/dev/null || true
pkill -x Xtigervnc 2>/dev/null || true
sleep 1

echo "[2/4] Cleaning X lock files..."
sudo rm -rf /tmp/.X11-unix /tmp/.X*-lock 2>/dev/null || true
sudo mkdir -p /tmp/.X11-unix
sudo chmod 1777 /tmp/.X11-unix

echo "[3/4] Starting Xtigervnc on :1 ..."
sudo -u vscode tigervncserver :1 \
    -geometry 1440x768 -depth 16 \
    -rfbport 5901 -dpi 96 \
    -localhost -desktop fluxbox \
    -SecurityTypes None -fg \
    &>/tmp/Xtigervnc.log &

for i in $(seq 1 10); do
    pgrep -x Xtigervnc &>/dev/null && break
    sleep 1
done
pgrep -x Xtigervnc &>/dev/null || { echo "ERROR: Xtigervnc failed to start. Check /tmp/Xtigervnc.log"; exit 1; }

echo "[4/4] Starting noVNC proxy on port 6080..."
sudo /usr/local/novnc/noVNC-1.6.0/utils/novnc_proxy \
    --listen 6080 --vnc localhost:5901 \
    &>/tmp/noVNC.log &
sleep 2

if ss -tlnp | grep -q 6080; then
    echo ""
    echo "Done. Open port 6080 in your browser (Ports tab in VS Code)."
else
    echo "ERROR: noVNC failed. Check /tmp/noVNC.log"
    cat /tmp/noVNC.log | tail -10
    exit 1
fi
