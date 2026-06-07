#!/system/bin/sh
# Start dockerd at boot on a LineageOS phone Stone (no systemd).
#
# Installed to /data/adb/service.d/dockerd.sh by install-dockerd-android.ps1. Magisk runs
# scripts here as root at "late_start" on every boot. Pairs with garden-moss-service.sh,
# which waits for `docker info` and then starts native garden-moss.
#
# Two-phase via self-re-exec under `unshare -m`:
#   outer: global prep (/dev/net/tun, tmpfs /run + /var) then re-exec into a mount namespace
#   --in-ns: build a cgroup v1 view of /sys/fs/cgroup (namespace-local), then start
#            containerd + dockerd inside it.
# Why cgroup v1: this kernel is 4.9, which predates BPF_CGROUP_DEVICE (4.15+). runc's
# cgroup v2 device controller uses eBPF and fails (`bpf_prog_query(BPF_CGROUP_DEVICE)`).
# cgroup v1's devices controller uses allow/deny files (no eBPF) and works on 4.9.

BIN=/data/docker/bin
LOG=/data/docker/dockerd-boot.log
export PATH=$BIN:/system/bin:/system/xbin:$PATH
export TMPDIR=/data/local/tmp
export DOCKER_HOST=unix:///data/docker/docker.sock

exec >>"$LOG" 2>&1

if [ "$1" = "--in-ns" ]; then
    echo "=== $(date) [mount-ns] cgroup v1 + daemons ==="
    # toybox `mount` cannot set mount propagation (--make-rprivate); the magisk busybox can.
    # Without private propagation our cgroup mounts leak back to Android and stack across runs.
    BB=/data/adb/magisk/busybox
    [ -x "$BB" ] || BB=""
    M() { if [ -n "$BB" ]; then "$BB" mount "$@"; else mount "$@"; fi; }
    U() { if [ -n "$BB" ]; then "$BB" umount "$@"; else umount "$@"; fi; }

    # CRITICAL: make this namespace's mounts private so nothing propagates back to Android.
    M --make-rprivate / 2>/dev/null

    # Shadow Android's cgroup2 with a tmpfs (namespace-local) and lay out a v1 hierarchy.
    M -t tmpfs -o mode=755 tmpfs /sys/fs/cgroup
    # Real v1 hierarchies for the controllers runc needs that are free ("hierarchy 0").
    # `devices` is the critical one: v2 device control needs eBPF (kernel >=4.15); this 4.9
    # kernel lacks it, so runc on v2 fails. v1 `devices` uses allow/deny files — works.
    for c in devices freezer; do
        mkdir -p "/sys/fs/cgroup/$c"
        M -t cgroup -o "$c" cgroup "/sys/fs/cgroup/$c" || echo "warn: mount cgroup $c failed"
    done

    # Hide Android's occupied v1 controllers from docker/runc in this private ns. They are
    # kernel-bound to Android's hierarchies (so we cannot re-mount them cleanly), binding them
    # is invasive, and cpuset uses legacy `noprefix` naming (files `cpus`/`mems`) runc can't
    # read. Unmounting them (ns-local) keeps Android's global view intact. Result: no
    # memory/cpu/blkio limits for containers — acceptable (offerings self-cap, e.g. Mongo).
    for m in /dev/cpuset /dev/cpuctl /dev/memcg /dev/blkio /dev/stune; do
        U -l "$m" 2>/dev/null
    done

    # runc still insists on a cpuset cgroup. It cannot be real-mounted (kernel-bound to
    # Android's noprefix hierarchy), so provide a tmpfs stand-in with the two files runc
    # reads/inherits (cpus/mems). No enforcement, but runc is satisfied. 8 cores on this SoC.
    mkdir -p /sys/fs/cgroup/cpuset
    echo "0-7" > /sys/fs/cgroup/cpuset/cpuset.cpus 2>/dev/null
    echo "0"   > /sys/fs/cgroup/cpuset/cpuset.mems 2>/dev/null

    # containerd standalone (state under /data), then dockerd against it. A hard reboot does
    # NOT remove the pidfile/socket, and dockerd refuses to start when docker.pid points at a
    # PID the kernel has since reused (intermittent "process with PID N is still running" on
    # boot) — so clean both stale artifacts before starting.
    if ! pidof containerd >/dev/null 2>&1; then
        rm -f /data/docker/exec/containerd/containerd.sock
        echo "starting containerd"
        nohup containerd --config /data/docker/containerd.toml >>/data/docker/containerd.log 2>&1 &
        i=0
        until [ -S /data/docker/exec/containerd/containerd.sock ]; do
            i=$((i + 1)); [ "$i" -ge 30 ] && break; sleep 1
        done
    fi
    rm -f /data/docker/docker.pid
    echo "starting dockerd"
    nohup dockerd --config-file /data/docker/daemon.json \
        --containerd /data/docker/exec/containerd/containerd.sock >>"$LOG" 2>&1 &
    echo "dockerd pid $!"
    exit 0
fi

# ── outer: global prep ───────────────────────────────────────────────────
echo "=== $(date) dockerd boot service ==="
if pidof dockerd >/dev/null 2>&1; then
    echo "dockerd already running"
    exit 0
fi

# Best-effort: ensure /dev/net/tun exists (some boots race this).
[ -e /dev/net/tun ] || { mkdir -p /dev/net; mknod /dev/net/tun c 10 200 2>/dev/null; }

# Android's / is read-only with no /run or /var, but dockerd/containerd hardcode several
# /run paths (e.g. /run/docker/plugins). Create the mountpoints (once, via a brief rw
# remount of /) and back them with tmpfs so those writes succeed. Idempotent across boots.
for d in /run /var; do
    [ -d "$d" ] || { mount -o remount,rw / 2>/dev/null; mkdir -p "$d" 2>/dev/null; mount -o remount,ro / 2>/dev/null; }
    ( touch "$d/.zgrw" 2>/dev/null && rm -f "$d/.zgrw" 2>/dev/null ) || mount -t tmpfs tmpfs "$d" 2>/dev/null
done
mkdir -p /var/run /run/docker
# moss's bollard uses connect_with_socket_defaults() which hardcodes /var/run/docker.sock
# and ignores DOCKER_HOST. Symlink it (on the tmpfs /var/run) to our /data socket so the
# native moss connects without a code change. dockerd creates the target on start.
ln -sf /data/docker/docker.sock /var/run/docker.sock

# Re-exec into a private mount namespace for the cgroup v1 setup + daemons.
echo "re-exec under unshare -m"
exec unshare -m sh "$0" --in-ns
