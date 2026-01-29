#!/bin/bash
# moss-update-helper.sh - Process pending upgrades before Moss starts
#
# Package structure mirrors target filesystem:
#   bin/  → /usr/local/bin/
#   lib/  → /var/lib/
#
# Flow:
# 1. push2all.ps1 uploads package to /var/lib/zen-garden/staging/pending-upgrade.tar.gz
# 2. On next Moss restart, this script extracts and installs the package
# 3. garden-upgrade.sh handles validated/ staging (for API-based upgrades)

set -euo pipefail

# Configuration
STAGING_DIR="/var/lib/zen-garden/staging"
PACKAGE_FILE="$STAGING_DIR/pending-upgrade.tar.gz"

log() {
    echo "[moss-update-helper] $1"
}

# Ensure staging directories exist with correct permissions
ensure_staging_dirs() {
    if [[ ! -d "$STAGING_DIR" ]]; then
        mkdir -p "$STAGING_DIR"
        chmod 755 "$STAGING_DIR"
        log "Created $STAGING_DIR"
    fi
}

# Process pending package upgrade
process_package_upgrade() {
    if [[ ! -f "$PACKAGE_FILE" ]]; then
        log "No pending upgrade package"
        return 0
    fi

    log "Found upgrade package: $PACKAGE_FILE"

    local work_dir
    work_dir=$(mktemp -d)
    trap 'rm -rf "$work_dir"' RETURN

    # Extract package
    if ! tar -xzf "$PACKAGE_FILE" -C "$work_dir"; then
        log "ERROR: Failed to extract package"
        rm -f "$PACKAGE_FILE"
        return 1
    fi

    # Find package directory (zen-garden-X.Y.Z-linux-amd64/)
    local pkg_dir
    pkg_dir=$(find "$work_dir" -maxdepth 1 -type d -name "zen-garden-*" | head -1)
    if [[ -z "$pkg_dir" ]]; then
        log "ERROR: Invalid package structure - no zen-garden-* directory found"
        rm -f "$PACKAGE_FILE"
        return 1
    fi

    log "Installing from: $(basename "$pkg_dir")"

    # Deploy bin/ → /usr/local/bin/
    if [[ -d "$pkg_dir/bin" ]]; then
        cp -r "$pkg_dir/bin/"* /usr/local/bin/
        find /usr/local/bin -type f -exec chmod 755 {} \;
        local bin_count
        bin_count=$(find "$pkg_dir/bin" -type f | wc -l)
        log "Deployed bin/ ($bin_count files)"
    fi

    # Deploy lib/ → /var/lib/
    if [[ -d "$pkg_dir/lib" ]]; then
        cp -r "$pkg_dir/lib/"* /var/lib/
        chown -R stone:stone /var/lib/zen-garden
        local lib_count
        lib_count=$(find "$pkg_dir/lib" -type f | wc -l)
        log "Deployed lib/ ($lib_count files)"
    fi

    # Apply garden configuration (timezone, NTP)
    if [[ -f /var/lib/zen-garden/garden.conf ]]; then
        # shellcheck source=/dev/null
        source /var/lib/zen-garden/garden.conf
        
        # Apply timezone if specified and different from current
        if [[ -n "${timezone:-}" ]]; then
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
    fi

    # Cleanup
    rm -f "$PACKAGE_FILE"
    rm -f "$STAGING_DIR"/*.staged 2>/dev/null || true
    
    log "Package upgrade complete"
}

# Main
main() {
    log "Starting update check..."

    ensure_staging_dirs
    process_package_upgrade

    log "Update check complete"
}

main
exit 0
