#!/bin/bash
# moss-update-helper.sh - Process pending upgrades before Moss starts
#
# This script runs as ExecStartPre in the systemd unit (before garden-upgrade.sh).
# It processes pending-upgrade.tar.gz packages deployed via push2all.ps1.
#
# Flow:
# 1. push2all.ps1 uploads package to /var/lib/zen-garden/staging/pending-upgrade.tar.gz
# 2. On next Moss restart, this script extracts and installs the package
# 3. garden-upgrade.sh handles validated/ staging (for API-based upgrades)

set -euo pipefail

# Configuration
STAGING_DIR="/var/lib/zen-garden/staging"
PACKAGE_FILE="$STAGING_DIR/pending-upgrade.tar.gz"
TARGET_DIR="/usr/local/bin"

log() {
    echo "[moss-update-helper] $1"
}

# Install dependencies from dependencies.json file
install_adapter_dependencies() {
    local deps_file="$1"
    
    if [[ ! -f "$deps_file" ]]; then
        log "No dependencies file found"
        return 0
    fi
    
    log "Processing dependencies from $(basename "$deps_file")..."
    
    # Check if jq is available for JSON parsing
    if ! command -v jq &>/dev/null; then
        log "WARNING: jq not installed, cannot process dependencies"
        return 0
    fi
    
    # Get list of adapters with apt dependencies
    local adapters
    adapters=$(jq -r '.linux | keys[]' "$deps_file" 2>/dev/null) || return 0
    
    for adapter in $adapters; do
        local adapter_binary="$TARGET_DIR/adapters/$adapter/garden-$adapter"
        
        # Only install dependencies if the adapter binary exists
        if [[ -f "$adapter_binary" ]]; then
            # Get apt packages for this adapter
            local packages
            packages=$(jq -r ".linux.\"$adapter\".apt // [] | .[]" "$deps_file" 2>/dev/null)
            local reason
            reason=$(jq -r ".linux.\"$adapter\".reason // \"required dependency\"" "$deps_file" 2>/dev/null)
            
            for pkg_spec in $packages; do
                # Handle alternative packages (pkg1 | pkg2)
                local installed=false
                # Split on | and try each package
                for pkg in $(echo "$pkg_spec" | tr '|' ' '); do
                    pkg=$(echo "$pkg" | xargs) # trim whitespace
                    if dpkg -s "$pkg" >/dev/null 2>&1; then
                        installed=true
                        break
                    fi
                done
                
                if [[ "$installed" == "false" ]]; then
                    # Try to install alternatives in order
                    for pkg in $(echo "$pkg_spec" | tr '|' ' '); do
                        pkg=$(echo "$pkg" | xargs) # trim whitespace
                        [[ -z "$pkg" ]] && continue  # skip empty strings
                        log "Installing $pkg for $adapter ($reason)..."
                        if apt-get install -y -qq "$pkg" 2>&1; then
                            log "Installed $pkg"
                            installed=true
                            break
                        fi
                    done
                    
                    if [[ "$installed" == "false" ]]; then
                        log "WARNING: Failed to install $pkg_spec - $adapter may not work"
                    fi
                fi
            done
        fi
    done
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

    # Deploy binaries (including adapters subdirectory)
    if [[ -d "$pkg_dir/bin" ]]; then
        # Ensure adapters directory exists
        mkdir -p "$TARGET_DIR/adapters"
        
        # Copy binaries
        for binary in "$pkg_dir/bin/"*; do
            if [[ -f "$binary" ]]; then
                local name
                name=$(basename "$binary")
                cp "$binary" "$TARGET_DIR/$name"
                chmod 755 "$TARGET_DIR/$name"
                log "Installed $name"
            fi
        done
        
        # Copy adapters subdirectory recursively
        if [[ -d "$pkg_dir/bin/adapters" ]]; then
            cp -r "$pkg_dir/bin/adapters/"* "$TARGET_DIR/adapters/"
            # Set permissions on all adapter files
            find "$TARGET_DIR/adapters" -type f -exec chmod 755 {} \;
            # Log installed adapters
            find "$pkg_dir/bin/adapters" -type f | while read -r adapter; do
                log "Installed adapters/$(basename "$(dirname "$adapter")")/$(basename "$adapter")"
            done
            # Install adapter dependencies from package
            if [[ -f "$pkg_dir/dependencies.json" ]]; then
                install_adapter_dependencies "$pkg_dir/dependencies.json"
            fi
        fi
    fi

    # Deploy manifests
    if [[ -d "$pkg_dir/manifests" ]]; then
        mkdir -p /var/lib/zen-garden/manifests
        cp -r "$pkg_dir/manifests/"* /var/lib/zen-garden/manifests/
        local manifest_count
        manifest_count=$(find "$pkg_dir/manifests" -type f | wc -l)
        log "Updated manifests ($manifest_count files)"
    fi

    # Deploy scripts
    if [[ -d "$pkg_dir/scripts" ]]; then
        for script in "$pkg_dir/scripts/"*.sh; do
            if [[ -f "$script" ]]; then
                local name
                name=$(basename "$script")
                cp "$script" "$TARGET_DIR/$name"
                chmod 755 "$TARGET_DIR/$name"
                log "Installed script $name"
            fi
        done
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
