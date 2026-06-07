#!/system/bin/sh
# Zen Garden Stone - Magisk boot service for native garden-moss (LineageOS phone, no systemd).
#
# Installed to /data/adb/service.d/garden-moss.sh. Magisk runs scripts here as root at
# "late_start" on every boot. Moss is the HOST management daemon (not a container) and runs
# as root directly on the device; it coordinates the host's dockerd over /var/run/docker.sock.
# Pairs with dockerd-service.sh, which starts dockerd first.

LOG=/data/garden-moss.log
BIN=/data/garden-moss

# dockerd binaries + standard tools on PATH; moss shells out to a few host tools.
export PATH=/data/docker/bin:/system/bin:/system/xbin:$PATH
# dockerd's socket lives on /data (no /var/run on Android); bollard honors DOCKER_HOST.
export DOCKER_HOST=unix:///data/docker/docker.sock
# Android has no writable /var or /etc tree — point moss at /data. The path constants read
# GARDEN_* (constants/paths.rs); ZG_* are kept as aliases for any EnvConfig accessors.
export GARDEN_DATA_DIR=/data/zen-garden
export GARDEN_CONFIG_DIR=/data/zen-garden/config
export GARDEN_COMPANIONS_DIR=/data/zen-garden/companions
export ZG_DATA_DIR=/data/zen-garden
export ZG_CONFIG_DIR=/data/zen-garden/config

exec >>"$LOG" 2>&1
echo "=== $(date) garden-moss boot service ==="

[ -x "$BIN" ] || { echo "FATAL: $BIN not found - run installer/deploy-android.ps1"; exit 1; }

if pidof garden-moss >/dev/null 2>&1; then
    echo "garden-moss already running"
    exit 0
fi

mkdir -p "$ZG_DATA_DIR" "$ZG_CONFIG_DIR" "$ZG_DATA_DIR/companions"

# (No /etc/zen-garden bind needed since HOST-0001: every former CONFIG_DIR-const writer
# now uses the env-honoring config_dir(), and first_run_flag() resolves under config_dir()
# too — so all config/cache/first-run state lands on /data directly.)

# Moss coordinates Docker; wait for the (hand-started) dockerd to be ready.
i=0
until docker info >/dev/null 2>&1; do
    i=$((i + 1))
    if [ "$i" -ge 60 ]; then
        echo "dockerd not up after ~120s; starting moss anyway (it will retry the socket)"
        break
    fi
    sleep 2
done

echo "starting garden-moss"
nohup "$BIN" >>"$LOG" 2>&1 &
echo "garden-moss pid $!"
