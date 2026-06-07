#!/system/bin/sh
# Verify dockerd is up on the phone Stone. Run as root via `su -c`.
export PATH=/data/docker/bin:/system/bin:/system/xbin:$PATH
export DOCKER_HOST=unix:///data/docker/docker.sock
echo "=== docker version ==="
docker version 2>&1 | head -20
echo "=== docker info (head) ==="
docker info 2>&1 | head -25
echo "=== dockerd log tail ==="
tail -n 40 /data/docker/dockerd-boot.log 2>/dev/null
tail -n 40 /data/docker/dockerd.log 2>/dev/null
echo "=== recent SELinux denials (avc) ==="
logcat -d 2>/dev/null | grep -i avc | tail -15
echo "VERIFY_DONE"
