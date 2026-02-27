#!/bin/bash
# setup-stone.sh - Bootstrap a fresh Debian machine into a Zen Garden stone
#
# Works with any Zen Garden package (x64 or x86).
# Run as root on the target machine after installing Debian.
#
# Usage:
#   sudo ./setup-stone.sh zen-garden-0.1.202602082356-linux-x86.tar.gz
#   sudo ./setup-stone.sh zen-garden-0.1.202602082356-linux-x64.tar.gz
#
# What this script does:
#   1. Extracts the package
#   2. Installs binaries to /usr/local/bin/
#   3. Installs systemd service and config files
#   4. Creates directories and sets permissions
#   5. Ensures 'stone' user exists with correct groups
#   6. Installs Docker if not present
#   7. Enables and starts garden-moss service
#
# Prerequisites:
#   - Debian-based Linux (Debian, Ubuntu)
#   - Root access
#   - Network connectivity (for Docker install if needed)

set -euo pipefail

# --- Configuration ---
STONE_USER="stone"
BIN_DIR="/usr/local/bin"
DATA_DIR="/var/lib/zen-garden"
CONFIG_DIR="/etc/zen-garden"
STAGING_DIR="$DATA_DIR/staging"

# --- Helpers ---
log()  { echo -e "\033[0;36m[setup-stone]\033[0m $1"; }
ok()   { echo -e "\033[0;32m  ✓\033[0m $1"; }
warn() { echo -e "\033[0;33m  !\033[0m $1"; }
fail() { echo -e "\033[0;31m  ✗\033[0m $1"; exit 1; }

# --- Checks ---
if [[ $EUID -ne 0 ]]; then
    fail "This script must be run as root (use sudo)"
fi

if [[ $# -lt 1 ]]; then
    echo "Usage: sudo ./setup-stone.sh <package.tar.gz>"
    echo ""
    echo "Example:"
    echo "  sudo ./setup-stone.sh zen-garden-0.1.202602082356-linux-x86.tar.gz"
    exit 1
fi

PACKAGE="$1"

if [[ ! -f "$PACKAGE" ]]; then
    fail "Package not found: $PACKAGE"
fi

# --- Extract package ---
log "Extracting package..."
WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

tar -xzf "$PACKAGE" -C "$WORK_DIR"

# Find the extracted directory (zen-garden-{version}-{platform}-{arch})
PACKAGE_DIR=$(find "$WORK_DIR" -maxdepth 1 -mindepth 1 -type d | head -1)
if [[ -z "$PACKAGE_DIR" ]]; then
    fail "Package extraction failed - no directory found"
fi

# Read package manifest
if [[ -f "$PACKAGE_DIR/package.json" ]]; then
    PACKAGE_VERSION=$(python3 -c "import json; print(json.load(open('$PACKAGE_DIR/package.json'))['version'])" 2>/dev/null || echo "unknown")
    PACKAGE_ARCH=$(python3 -c "import json; print(json.load(open('$PACKAGE_DIR/package.json')).get('architecture', 'unknown'))" 2>/dev/null || echo "unknown")
    ok "Package: v$PACKAGE_VERSION ($PACKAGE_ARCH)"
else
    PACKAGE_VERSION="unknown"
    PACKAGE_ARCH="unknown"
    warn "No package.json found, proceeding anyway"
fi

# --- Ensure stone user ---
log "Checking user '$STONE_USER'..."
if id "$STONE_USER" &>/dev/null; then
    ok "User '$STONE_USER' exists"
else
    log "Creating user '$STONE_USER'..."
    useradd -m -s /bin/bash "$STONE_USER"
    echo "$STONE_USER:$STONE_USER" | chpasswd
    ok "Created user '$STONE_USER'"
fi

# Add to sudo and docker groups
usermod -aG sudo "$STONE_USER" 2>/dev/null || true
if ! grep -q "^$STONE_USER" /etc/sudoers.d/"$STONE_USER" 2>/dev/null; then
    echo "$STONE_USER ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/"$STONE_USER"
    chmod 0440 /etc/sudoers.d/"$STONE_USER"
    ok "Passwordless sudo configured"
fi

# --- Create directories ---
log "Creating directories..."
mkdir -p "$BIN_DIR" "$DATA_DIR" "$CONFIG_DIR" "$STAGING_DIR" "/home/$STONE_USER/bin" "/etc/netplan"
ok "Directories ready"

# --- Install binaries ---
log "Installing binaries..."
if [[ -d "$PACKAGE_DIR/bin" ]]; then
    INSTALLED=0
    # Copy all files from bin/
    find "$PACKAGE_DIR/bin" -type f | while read -r file; do
        rel="${file#$PACKAGE_DIR/bin/}"
        dest="$BIN_DIR/$rel"
        dest_dir=$(dirname "$dest")
        mkdir -p "$dest_dir"
        cp "$file" "$dest"
        chmod 755 "$dest"
        ok "$rel → $dest"
    done
else
    warn "No bin/ directory in package"
fi

# --- Install scripts (filesystem-mirrored paths) ---
log "Installing scripts..."
NEEDS_DAEMON_RELOAD=false
if [[ -d "$PACKAGE_DIR/scripts" ]]; then
    while IFS= read -r -d '' file; do
        rel="${file#$PACKAGE_DIR/scripts/}"
        target="/$rel"
        target_dir=$(dirname "$target")
        mkdir -p "$target_dir"
        cp "$file" "$target"

        case "$target" in
            /etc/systemd/system/*)
                chmod 644 "$target"
                NEEDS_DAEMON_RELOAD=true
                ;;
            /usr/local/bin/*)
                chmod 755 "$target"
                ;;
            /var/lib/zen-garden/*)
                chown "$STONE_USER:$STONE_USER" "$target" 2>/dev/null || true
                ;;
        esac
        ok "$rel → $target"
    done < <(find "$PACKAGE_DIR/scripts" -type f -print0)
else
    warn "No scripts/ directory in package"
fi

# --- Ensure garden-moss.toml exists ---
MOSS_CONFIG="$CONFIG_DIR/garden-moss.toml"
if [[ ! -f "$MOSS_CONFIG" ]]; then
    log "Creating default garden-moss.toml..."
    cat > "$MOSS_CONFIG" << 'TOMLEOF'
# garden-moss configuration

port = 7185
log_level = "info"
TOMLEOF
    chown "$STONE_USER:$STONE_USER" "$MOSS_CONFIG" 2>/dev/null || true
    ok "Default config created"
else
    ok "garden-moss.toml already exists"
fi

# --- Install Docker if needed ---
log "Checking Docker..."
if command -v docker &>/dev/null; then
    ok "Docker is installed"
else
    log "Installing Docker..."
    if command -v apt-get &>/dev/null; then
        apt-get update -qq
        apt-get install -y -qq docker.io >/dev/null 2>&1
        ok "Docker installed via apt"
    else
        warn "apt-get not found, install Docker manually"
    fi
fi

# Enable Docker and add stone to docker group
if command -v docker &>/dev/null; then
    systemctl enable docker 2>/dev/null || true
    systemctl start docker 2>/dev/null || true
    usermod -aG docker "$STONE_USER" 2>/dev/null || true
    ok "Docker enabled, '$STONE_USER' added to docker group"
fi

# --- Set ownership ---
log "Setting permissions..."
chown -R "$STONE_USER:$STONE_USER" "/home/$STONE_USER" 2>/dev/null || true
chown "$STONE_USER:$STONE_USER" "$CONFIG_DIR" 2>/dev/null || true
chown "$STONE_USER:$STONE_USER" "$DATA_DIR" 2>/dev/null || true
ok "Permissions set"

# --- Apply garden config ---
if [[ -f "$DATA_DIR/garden.conf" ]]; then
    log "Applying garden configuration..."
    # shellcheck source=/dev/null
    source "$DATA_DIR/garden.conf"
    if [[ -n "${timezone:-}" ]]; then
        timedatectl set-timezone "$timezone" 2>/dev/null && ok "Timezone: $timezone" || warn "Failed to set timezone"
    fi
    timedatectl set-ntp true 2>/dev/null && ok "NTP enabled" || warn "Failed to enable NTP"
fi

# --- Enable avahi for discovery ---
log "Checking mDNS/discovery..."
if command -v avahi-daemon &>/dev/null; then
    systemctl enable avahi-daemon 2>/dev/null || true
    systemctl start avahi-daemon 2>/dev/null || true
    ok "avahi-daemon enabled"
else
    if command -v apt-get &>/dev/null; then
        apt-get install -y -qq avahi-daemon >/dev/null 2>&1 && ok "avahi-daemon installed" || warn "Failed to install avahi-daemon"
        systemctl enable avahi-daemon 2>/dev/null || true
        systemctl start avahi-daemon 2>/dev/null || true
    else
        warn "avahi-daemon not found, stone discovery may not work"
    fi
fi

# --- Enable systemd-resolved + systemd-networkd for DNS ---
log "Configuring systemd-resolved..."
if ! command -v resolvectl &>/dev/null; then
    if command -v apt-get &>/dev/null; then
        apt-get install -y -qq systemd-resolved >/dev/null 2>&1 \
            && ok "systemd-resolved installed" \
            || warn "Failed to install systemd-resolved"
    fi
fi

if command -v resolvectl &>/dev/null; then
    mkdir -p /etc/systemd/resolved.conf.d
    cat > /etc/systemd/resolved.conf.d/zen-garden.conf << 'RESOLVED_EOF'
[Resolve]
# Handle .local mDNS queries (resolve-only, avahi handles publishing)
MulticastDNS=resolve
RESOLVED_EOF

    systemctl enable systemd-resolved 2>/dev/null || true
    systemctl restart systemd-resolved 2>/dev/null || true

    # systemd-networkd provides the D-Bus interface that resolvectl needs
    # for per-interface DNS/domain routing (e.g. .zengarden → Koi DNS).
    # Bundled in the systemd package on Debian — just enable it.
    systemctl enable systemd-networkd 2>/dev/null || true
    systemctl start systemd-networkd 2>/dev/null || true

    # Mask the wait-online service: systemd-networkd is only here for the
    # D-Bus interface that resolvectl needs — it doesn't manage interfaces
    # (ifupdown does), so wait-online has nothing to wait on and would
    # block boot for 2 minutes. Masking it is safe because
    # network-online.target uses Wants= (not Requires=) so it still
    # activates fine, and moss handles network readiness internally.
    systemctl mask systemd-networkd-wait-online.service 2>/dev/null || true

    # Point /etc/resolv.conf to resolved stub
    if [ ! -L /etc/resolv.conf ] || [ "$(readlink /etc/resolv.conf)" != "/run/systemd/resolve/stub-resolv.conf" ]; then
        ln -sf /run/systemd/resolve/stub-resolv.conf /etc/resolv.conf
    fi

    ok "systemd-resolved + systemd-networkd configured"
else
    warn "systemd-resolved not available, container DNS may not resolve stone names"
fi

# --- Enable and start service ---
log "Configuring garden-moss service..."
if [[ "$NEEDS_DAEMON_RELOAD" == true ]]; then
    systemctl daemon-reload
    ok "systemd reloaded"
fi

if [[ -f /etc/systemd/system/garden-moss.service ]]; then
    systemctl enable garden-moss.service
    ok "garden-moss.service enabled"

    log "Starting garden-moss..."
    systemctl start garden-moss.service
    sleep 2

    if systemctl is-active --quiet garden-moss.service; then
        ok "garden-moss is running"
    else
        warn "garden-moss failed to start - check: journalctl -u garden-moss -n 20"
    fi
else
    warn "garden-moss.service not found in package"
fi

# --- Summary ---
echo ""
echo -e "\033[0;32m╔════════════════════════════════════════════════════╗\033[0m"
echo -e "\033[0;32m║  Stone Setup Complete                              ║\033[0m"
echo -e "\033[0;32m╚════════════════════════════════════════════════════╝\033[0m"
echo ""
echo "  Version:      $PACKAGE_VERSION"
echo "  Architecture: $PACKAGE_ARCH"
echo "  User:         $STONE_USER"
echo "  Binaries:     $BIN_DIR"
echo "  Data:         $DATA_DIR"
echo "  Config:       $CONFIG_DIR"
echo ""
echo "  Useful commands:"
echo "    journalctl -u garden-moss -f        # Follow logs"
echo "    systemctl status garden-moss        # Service status"
echo "    systemctl restart garden-moss       # Restart"
echo ""
