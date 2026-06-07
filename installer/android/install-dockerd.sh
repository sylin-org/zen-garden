#!/system/bin/sh
# Install static Docker on a rooted LineageOS phone Stone (the "Stage 3a" bring-up).
#
# Pushed to /data/local/tmp/ alongside the docker static bundle + runc, then run as root
# via `su -c`. Idempotent: re-running reinstalls binaries and rewrites config.
#
# Args (optional):
#   $1 docker static bundle tgz   (default: /data/local/tmp/docker.tgz)
#   $2 runc binary to pin         (default: /data/local/tmp/runc.arm64, runc 1.1.12)

set -e
BUNDLE="${1:-/data/local/tmp/docker.tgz}"
RUNC="${2:-/data/local/tmp/runc.arm64}"

ROOT=/data/docker
BIN="$ROOT/bin"
DATA="$ROOT/data"

# NOTE: /etc is read-only on Android (symlink into /system) — daemon config lives under /data.
mkdir -p "$BIN" "$DATA"

echo "Extracting docker bundle: $BUNDLE"
[ -f "$BUNDLE" ] || { echo "FATAL: bundle not found: $BUNDLE"; exit 10; }
tmp="$ROOT/.unpack"
rm -rf "$tmp"; mkdir -p "$tmp"
# gzip | tar (toybox tar -z is unreliable; gzip is present and explicit).
gzip -dc "$BUNDLE" | tar -x -C "$tmp"
cp -f "$tmp"/docker/* "$BIN"/
rm -rf "$tmp"

# Pin runc to the researched-safe version (newer runc/containerd can hang on this kernel).
if [ -f "$RUNC" ]; then
    cp -f "$RUNC" "$BIN/runc"
    echo "Pinned runc from $RUNC"
fi
chmod 0755 "$BIN"/*

echo "Installed:"; ls -1 "$BIN"
echo "runc version:"; "$BIN/runc" --version 2>&1 | head -1

# Daemon config under /data (no writable /etc, no /var/run on Android).
#  - hosts: socket on /data (the default /var/run/docker.sock path does not exist here);
#    moss and the docker CLI reach it via DOCKER_HOST=unix:///data/docker/docker.sock.
#  - iptables=false + bridge=none: no NAT layer. Docker's normal bridge + -p publishing
#    does NOT work here even though the kernel has CONFIG_BRIDGE / VETH / NF_NAT (verified):
#    Android's per-network policy routing leaves the host unable to route to docker0
#    containers, so published ports are unreachable from the LAN. Offerings therefore use
#    host networking (HostProfile defaults container network_mode=Host on Android), which
#    binds each offering's ports directly on the host stack — fire-and-forget LAN reach.
cat > "$ROOT/daemon.json" <<'EOF'
{
  "data-root": "/data/docker/data",
  "exec-root": "/data/docker/exec",
  "pidfile": "/data/docker/docker.pid",
  "hosts": ["unix:///data/docker/docker.sock"],
  "iptables": false,
  "ip6tables": false,
  "bridge": "none",
  "storage-driver": "overlay2",
  "exec-opts": ["native.cgroupdriver=cgroupfs"]
}
EOF
echo "Wrote $ROOT/daemon.json"

# Standalone containerd config — its defaults (root /var/lib/containerd, state /run/containerd)
# hit Android's read-only /var and /run. dockerd is started with --containerd pointing here.
cat > "$ROOT/containerd.toml" <<'EOF'
version = 2
root = "/data/docker/data/containerd"
state = "/data/docker/exec/containerd"
[grpc]
  address = "/data/docker/exec/containerd/containerd.sock"
[debug]
  address = "/data/docker/exec/containerd/debug.sock"
  level = "info"
EOF
echo "Wrote $ROOT/containerd.toml"

# Convenience wrapper so `rake ...` runs the native garden-rake from any terminal
# (garden-rake is a static-musl binary that runs directly on Android).
cat > "$BIN/rake" <<'EOF'
#!/system/bin/sh
exec /data/garden-rake "$@"
EOF
chmod 0755 "$BIN/rake"

echo "INSTALL_OK"
