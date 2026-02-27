#!/bin/bash
# moss-update-helper.sh - Process validated upgrades before Moss starts
#
# Flow:
# 1. deploy.ps1 sends package to Moss HTTP API (/api/v1/stone/deploy)
# 2. Moss validates and extracts to /var/lib/zen-garden/staging/validated/
# 3. Moss triggers service restart
# 4. This script (ExecStartPre) installs from validated/ before Moss starts
#
# Package structure in validated/:
#   bin/      → /usr/local/bin/     (full mirror copy)
#   scripts/  → filesystem paths    (scripts/X/Y/Z → /X/Y/Z)
#
# Post-install hooks:
#   - /etc/systemd/system/* → systemctl daemon-reload
#   - /usr/local/bin/*      → chmod 755
#   - /var/lib/zen-garden/* → chown stone:stone

set -euo pipefail

STAGING_DIR="/var/lib/zen-garden/staging"
VALIDATED_DIR="$STAGING_DIR/validated"

log() {
    echo "[moss-update-helper] $1"
}

# Write progress to TTY1 for physical console visibility
# Falls back silently if /dev/tty1 is not available
tty_log() {
    echo "[update] $1" > /dev/tty1 2>/dev/null || true
}

ensure_staging_dirs() {
    if [[ ! -d "$STAGING_DIR" ]]; then
        mkdir -p "$STAGING_DIR"
        chmod 755 "$STAGING_DIR"
        log "Created $STAGING_DIR"
    fi
}

# Deploy bin/ → /usr/local/bin/ (full mirror copy)
deploy_bin() {
    local src_dir="$1"

    if [[ ! -d "$src_dir" ]]; then
        return 0
    fi

    cp -r "$src_dir/"* /usr/local/bin/

    # Make all files executable
    find /usr/local/bin -maxdepth 1 -type f -exec chmod 755 {} \;

    # Handle subdirectories (companions, etc.)
    if [[ -d /usr/local/bin/companions ]]; then
        find /usr/local/bin/companions -type f -exec chmod 755 {} \;
    fi

    local count
    count=$(find "$src_dir" -type f | wc -l)
    log "Deployed bin/ ($count files) → /usr/local/bin/"
    tty_log "  ✓ Binaries installed ($count files)"
}

# Deploy scripts/ → filesystem paths (traversal)
deploy_scripts() {
    local src_dir="$1"
    local needs_daemon_reload=false

    if [[ ! -d "$src_dir" ]]; then
        return 0
    fi

    log "Deploying scripts/ (filesystem-mirrored paths)..."

    # Find all files in scripts/ and copy to their mirror paths
    while IFS= read -r -d '' file; do
        # Get relative path from scripts/
        local rel_path="${file#$src_dir/}"
        local target_path="/$rel_path"
        local target_dir
        target_dir=$(dirname "$target_path")

        # Ensure target directory exists
        mkdir -p "$target_dir"

        # Copy file
        cp "$file" "$target_path"
        log "  $rel_path → $target_path"

        # Apply post-install hooks based on path
        case "$target_path" in
            /etc/systemd/system/*)
                needs_daemon_reload=true
                chmod 644 "$target_path"
                ;;
            /usr/local/bin/*)
                chmod 755 "$target_path"
                ;;
            /var/lib/zen-garden/*)
                chown stone:stone "$target_path" 2>/dev/null || true
                ;;
        esac
    done < <(find "$src_dir" -type f -print0)

    # Run daemon-reload if any systemd files were updated
    if [[ "$needs_daemon_reload" == true ]]; then
        log "Running systemctl daemon-reload..."
        systemctl daemon-reload || log "WARNING: daemon-reload failed"
    fi

    local count
    count=$(find "$src_dir" -type f | wc -l)
    log "Deployed scripts/ ($count files)"
    tty_log "  ✓ Scripts deployed ($count files)"
}

# Apply garden configuration (timezone, NTP)
apply_garden_config() {
    if [[ ! -f /var/lib/zen-garden/garden.conf ]]; then
        return 0
    fi

    # shellcheck source=/dev/null
    source /var/lib/zen-garden/garden.conf

    # Apply timezone if specified and different from current
    if [[ -n "${timezone:-}" ]]; then
        local current_tz
        current_tz=$(timedatectl show --property=Timezone --value 2>/dev/null || echo "unknown")
        if [[ "$current_tz" != "$timezone" ]]; then
            log "Setting timezone: $timezone (was: $current_tz)"
            timedatectl set-timezone "$timezone" || log "WARNING: Failed to set timezone"
        fi
    fi

    # Ensure NTP is enabled
    if ! timedatectl show --property=NTP --value 2>/dev/null | grep -q "yes"; then
        log "Enabling NTP time synchronization"
        timedatectl set-ntp true || log "WARNING: Failed to enable NTP"
    fi
}

process_validated_upgrade() {
    if [[ ! -d "$VALIDATED_DIR/bin" ]]; then
        log "No validated upgrade pending"
        return 0
    fi

    log "Found validated upgrade in: $VALIDATED_DIR"
    tty_log "Installing update..."

    # Deploy bin/ → /usr/local/bin/
    deploy_bin "$VALIDATED_DIR/bin"

    # Deploy scripts/ → filesystem paths
    deploy_scripts "$VALIDATED_DIR/scripts"

    # Apply garden configuration
    apply_garden_config

    # Cleanup validated staging
    rm -rf "$VALIDATED_DIR"
    log "Upgrade complete"
    tty_log "  ✓ Update complete, starting new version..."
}

# One-time remediation: fix 2-minute boot delay from systemd-networkd-wait-online
# systemd-networkd is only running for resolved D-Bus support — it doesn't manage
# interfaces (ifupdown does), so wait-online blocks boot for 2 min with nothing
# to wait on. Masking it is safe: network-online.target uses Wants= not Requires=.
fix_wait_online() {
    # Skip if already masked
    if systemctl is-enabled systemd-networkd-wait-online.service 2>/dev/null | grep -q masked; then
        return 0
    fi

    # Only fix if systemd-networkd is enabled (we caused this in setup-stone.sh)
    if ! systemctl is-enabled systemd-networkd &>/dev/null; then
        return 0
    fi

    log "Masking systemd-networkd-wait-online (one-time boot fix)"
    systemctl mask systemd-networkd-wait-online.service 2>/dev/null || true

    # Clean up the old timeout override if it exists from a previous deploy
    rm -rf /etc/systemd/system/systemd-networkd-wait-online.service.d 2>/dev/null || true
    systemctl daemon-reload 2>/dev/null || true
}

main() {
    log "Starting update check..."
    ensure_staging_dirs
    process_validated_upgrade
    fix_wait_online
    log "Update check complete"
}

main
exit 0
