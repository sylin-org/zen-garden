#!/system/bin/sh
# Native garden-moss BOOTSTRAP on the phone Stone (run as root via `su -c`).
#
# One-time adb bootstrap only — all SUBSEQUENT updates go through the standard HTTP deploy path
# (POST /api/v1/stone/deploy), applied by `garden-moss pre-start` + the watchdog (DEPLOY-0001).
# Moss is the host management daemon and runs natively (static-musl ELF), not in a container.
# Expects pushed to /data/local/tmp/: garden-moss, garden-rake, garden-moss-service.sh.

SRC=/data/local/tmp
BIN_DIR=/data/zen-garden/bin          # = HostProfile.paths.bin_install (Android default)
BIN=$BIN_DIR/garden-moss
LOCK=/data/zen-garden/moss-watchdog.pid
export PATH=/data/docker/bin:/system/bin:/system/xbin:$PATH

[ -f "$SRC/garden-moss" ] || { echo "FATAL: $SRC/garden-moss missing (push it first)"; exit 10; }

mkdir -p "$BIN_DIR" /data/zen-garden/config /data/zen-garden/companions

# Stop any running watchdog + moss before swapping the binary.
if [ -f "$LOCK" ]; then
    wpid=$(cat "$LOCK" 2>/dev/null)
    [ -n "$wpid" ] && kill "$wpid" 2>/dev/null
    rm -f "$LOCK"
fi
pid=$(pidof garden-moss 2>/dev/null)
[ -n "$pid" ] && kill $pid 2>/dev/null
sleep 1

# Install binaries to bin_install.
cp -f "$SRC/garden-moss" "$BIN" || { echo "FATAL: cp garden-moss failed"; exit 11; }
chmod 0755 "$BIN"
if [ -f "$SRC/garden-rake" ]; then
    cp -f "$SRC/garden-rake" "$BIN_DIR/garden-rake" && chmod 0755 "$BIN_DIR/garden-rake"
fi
# Remove the legacy flat location if present (superseded by bin_install).
rm -f /data/garden-moss /data/garden-rake 2>/dev/null

# Install the Magisk boot service (now the watchdog).
mkdir -p /data/adb/service.d
cp -f "$SRC/garden-moss-service.sh" /data/adb/service.d/garden-moss.sh
chmod 0755 /data/adb/service.d/garden-moss.sh
echo "Installed /data/adb/service.d/garden-moss.sh (watchdog)"

# Launch the watchdog in the BACKGROUND (it runs moss in the foreground + respawns, so it must
# not block this bootstrap). On boot, Magisk launches it the same way from service.d.
nohup sh /data/adb/service.d/garden-moss.sh >/dev/null 2>&1 &

echo "Waiting for moss on :7185 ..."
i=0
while [ "$i" -lt 30 ]; do
    if curl -fsS http://127.0.0.1:7185/health >/dev/null 2>&1 || wget -qO- http://127.0.0.1:7185/health >/dev/null 2>&1; then
        break
    fi
    i=$((i + 1))
    sleep 1
done

echo "--- moss version ---"
"$BIN" --version 2>&1 | head -1
echo "--- :7185 listening? ---"
netstat -tlnp 2>/dev/null | grep 7185 || echo "(7185 not listening yet)"
echo "--- :7185 health ---"
{ curl -fsS http://127.0.0.1:7185/health 2>/dev/null || wget -qO- http://127.0.0.1:7185/health 2>/dev/null; } || echo "(no health response yet)"
echo "--- moss log tail ---"
tail -n 50 /data/garden-moss.log 2>/dev/null
echo "DEPLOY_OK"
