#!/system/bin/sh
# Native garden-moss deploy on the phone Stone (run as root via `su -c`).
#
# Moss is the host management daemon and runs natively (static-musl ELF), not in a container.
# Expects these already pushed to /data/local/tmp/: garden-moss, garden-rake,
# garden-moss-service.sh. Installs them, registers the Magisk boot service, and (re)starts moss.

SRC=/data/local/tmp
export PATH=/data/docker/bin:/system/bin:/system/xbin:$PATH

if [ ! -f "$SRC/garden-moss" ]; then
    echo "FATAL: $SRC/garden-moss missing (push it first)"; exit 10
fi

# Install binaries
cp -f "$SRC/garden-moss" /data/garden-moss || { echo "FATAL: cp garden-moss failed"; exit 11; }
chmod 0755 /data/garden-moss
if [ -f "$SRC/garden-rake" ]; then
    cp -f "$SRC/garden-rake" /data/garden-rake && chmod 0755 /data/garden-rake
fi

mkdir -p /data/zen-garden /data/zen-garden/config

# Install Magisk boot service
mkdir -p /data/adb/service.d
cp -f "$SRC/garden-moss-service.sh" /data/adb/service.d/garden-moss.sh
chmod 0755 /data/adb/service.d/garden-moss.sh
echo "Installed /data/adb/service.d/garden-moss.sh"

# Restart moss
pid=$(pidof garden-moss 2>/dev/null)
[ -n "$pid" ] && kill $pid 2>/dev/null
sleep 1
sh /data/adb/service.d/garden-moss.sh

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
/data/garden-moss --version 2>&1 | head -1
echo "--- :7185 listening? ---"
netstat -tlnp 2>/dev/null | grep 7185 || echo "(7185 not listening yet)"
echo "--- :7185 health ---"
{ curl -fsS http://127.0.0.1:7185/health 2>/dev/null || wget -qO- http://127.0.0.1:7185/health 2>/dev/null; } || echo "(no health response yet)"
echo "--- moss log tail ---"
tail -n 50 /data/garden-moss.log 2>/dev/null
echo "DEPLOY_OK"
