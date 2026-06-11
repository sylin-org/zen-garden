#!/system/bin/sh
# Zen Garden Stone - Magisk boot service + WATCHDOG for native garden-moss (LineageOS, no systemd).
#
# Installed to /data/adb/service.d/garden-moss.sh. Magisk runs scripts here as root at
# "late_start" (a background service slot, so the loop below does not block boot). This is the
# Android arm of DEPLOY-0001's per-platform supervisor: on every moss exit it applies any staged
# upgrade (`garden-moss pre-start`) and respawns — the role systemd/`Restart=always` plays on Linux.
#
# Exit-code contract (garden-common constants::server::exit), branched below:
#   0  STOP          -> operator stop / uninstall; watchdog stops.
#   10 RESTART_APPLY -> staged upgrade pending; pre-start applied it; respawn.
#   11 RESTART       -> restart requested (no payload); respawn.
#   *  FATAL/crash   -> respawn with exponential backoff; roll back from the rollback snapshot after repeated fast crashes.

LOG=/data/garden-moss.log
# Align with HostProfile.paths.bin_install (Android default = /data/zen-garden/bin). pre-start
# applies staged binaries here, so the watchdog must run moss from the same path.
BIN=/data/zen-garden/bin/garden-moss
LOCK=/data/zen-garden/moss-watchdog.pid
# Lock DIRECTORY for the atomic single-instance guard below (created with `mkdir`, which is atomic).
# Kept separate from the .pid file above (the pidfile stays informational and is how
# deploy-moss-native.sh finds + stops a running watchdog). The dir records its holder's boot-id and
# pid so a lock left behind by a hard reboot is detectable as stale.
GUARD=/data/zen-garden/moss-watchdog.lock
# DEPLOY-0001 rollback snapshot: `garden-moss pre-start` stashes the binaries it replaces here,
# OUTSIDE bin_install + companions (so nothing scans/launches a backup). Backups mirror their
# absolute path, so the moss backup is $ROLLBACK followed by $BIN.
ROLLBACK=/data/zen-garden/rollback
MOSS_BACKUP="$ROLLBACK$BIN"

# dockerd binaries + standard tools on PATH; moss shells out to a few host tools.
export PATH=/data/docker/bin:/system/bin:/system/xbin:$PATH
# dockerd's socket lives on /data (no /var/run on Android); bollard honors DOCKER_HOST.
export DOCKER_HOST=unix:///data/docker/docker.sock
# Android has no writable /var or /etc tree — point moss at /data.
export GARDEN_DATA_DIR=/data/zen-garden
export GARDEN_CONFIG_DIR=/data/zen-garden/config
export GARDEN_COMPANIONS_DIR=/data/zen-garden/companions
export ZG_DATA_DIR=/data/zen-garden
export ZG_CONFIG_DIR=/data/zen-garden/config

exec >>"$LOG" 2>&1
echo "=== $(date) garden-moss watchdog boot ==="

mkdir -p "$ZG_DATA_DIR" "$ZG_CONFIG_DIR" "$ZG_DATA_DIR/companions" "$(dirname "$BIN")"

# One-time migration from the legacy flat location to bin_install.
if [ ! -x "$BIN" ] && [ -x /data/garden-moss ]; then
    echo "migrating /data/garden-moss -> $BIN"
    mv -f /data/garden-moss "$BIN"
fi

# Atomic, reboot-safe single-instance guard. `mkdir` is atomic, so two concurrent Magisk
# boot-triggers can't both win the lock (the old `[ -f LOCK ] && kill -0` check was check-then-act
# and allowed a double-spawn). The lock dir survives a hard reboot (the EXIT trap can't run when the
# kernel kills us at shutdown), so on a failed mkdir the lock is STALE unless its recorded boot-id
# matches the current boot AND its pid is still alive — otherwise the pid is a leftover from a prior
# boot (since recycled — the bug this fixes: `kill -0` on the recycled pid wrongly read as "running")
# or a killed instance; clear it and retry. (flock-on-fd is unusable here: the phone's /system/bin/sh
# is mksh, which marks exec-opened fds close-on-exec, so the locked fd never reaches the flock child.)
bootid=$(cat /proc/sys/kernel/random/boot_id 2>/dev/null)
until mkdir "$GUARD" 2>/dev/null; do
    oldboot=$(cat "$GUARD/boot_id" 2>/dev/null)
    oldpid=$(cat "$GUARD/pid" 2>/dev/null)
    if [ -n "$bootid" ] && [ "$oldboot" = "$bootid" ] && [ -n "$oldpid" ] && kill -0 "$oldpid" 2>/dev/null; then
        echo "watchdog already running (pid $oldpid) — exiting"
        exit 0
    fi
    echo "clearing stale watchdog lock (pid ${oldpid:-none}; prior boot or dead holder)"
    rm -rf "$GUARD" || { echo "FATAL: cannot clear stale lock $GUARD"; exit 1; }
done
printf '%s\n' "$bootid" >"$GUARD/boot_id"
echo $$ >"$GUARD/pid"
# Mirror the pid to the legacy pidfile so deploy-moss-native.sh can still find + stop the watchdog.
echo $$ >"$LOCK"
trap 'rm -rf "$GUARD"; rm -f "$LOCK"' EXIT

[ -x "$BIN" ] || { echo "FATAL: $BIN not found — run installer/deploy-android.ps1 to bootstrap"; exit 1; }

# Moss coordinates Docker; wait once for the (hand-started) dockerd before the first launch.
i=0
until docker info >/dev/null 2>&1; do
    i=$((i + 1))
    if [ "$i" -ge 60 ]; then
        echo "dockerd not up after ~120s; starting moss anyway (it retries the socket)"
        break
    fi
    sleep 2
done

# ── Watchdog loop ────────────────────────────────────────────────────────────
crashes=0   # consecutive fast crashes
backoff=1   # seconds, exponential, capped
while :; do
    # Apply any staged upgrade (no-op when nothing is staged). This is the Android equivalent of
    # systemd ExecStartPre. It writes to bin_install via rename-aside, stashing the previous
    # binaries in the rollback snapshot ($ROLLBACK) for the crash-loop recovery below.
    "$BIN" pre-start

    start=$(date +%s)
    "$BIN"
    rc=$?
    ran=$(( $(date +%s) - start ))
    echo "=== $(date) moss exited rc=$rc after ${ran}s ==="

    case "$rc" in
        0)
            echo "clean STOP (rc=0) — watchdog stopping"
            break
            ;;
        10 | 11)
            echo "restart requested (rc=$rc) — reapply + respawn"
            crashes=0
            backoff=1
            continue
            ;;
    esac

    # Anything else is a crash/FATAL. A long run resets the crash-loop counter.
    if [ "$ran" -ge 60 ]; then
        crashes=0
        backoff=1
    fi
    crashes=$((crashes + 1))
    echo "crash rc=$rc (consecutive=$crashes)"

    # Roll back to the previous binary after repeated fast crashes (a bad self-update). pre-start
    # stashed the previous moss in the rollback snapshot; a healthy moss deletes the snapshot
    # (mark-good), so this only fires for a binary that never became healthy.
    if [ "$crashes" -ge 3 ] && [ -f "$MOSS_BACKUP" ]; then
        echo "ROLLBACK: restoring $MOSS_BACKUP after $crashes fast crashes"
        mv -f "$MOSS_BACKUP" "$BIN"
        chmod 755 "$BIN"
        crashes=0
        backoff=1
        continue
    fi

    echo "backoff ${backoff}s"
    sleep "$backoff"
    backoff=$((backoff * 2))
    [ "$backoff" -gt 60 ] && backoff=60
done
