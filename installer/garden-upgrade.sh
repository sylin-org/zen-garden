#!/bin/bash
# garden-upgrade.sh - Install pre-validated binaries from staging
#
# This script runs as ExecStartPre in the systemd unit.
# It installs binaries that have already been validated by the Moss service.
#
# Staging structure (created by Moss):
#   /var/lib/zen-garden/staging/validated/
#   ├── bin/
#   │   ├── garden-moss
#   │   ├── garden-rake
#   │   └── garden-lantern (optional)
#   └── scripts/
#       ├── moss-update-helper.sh
#       └── garden-upgrade.sh
#
# SAFETY: This script can upgrade itself. Linux allows overwriting a running
# script's file because the running process holds the inode. Next restart will
# use the new version.

set -euo pipefail

STAGING_DIR="/var/lib/zen-garden/staging/validated"
TARGET_BIN="/usr/local/bin"

log() {
    echo "[garden-upgrade] $1"
}

# Exit early if no staged content
if [[ ! -d "$STAGING_DIR" ]]; then
    exit 0
fi

if [[ ! -d "$STAGING_DIR/bin" ]] && [[ ! -d "$STAGING_DIR/scripts" ]]; then
    log "Staging directory exists but is empty"
    exit 0
fi

log "Installing staged binaries..."

# Install scripts FIRST (so we can upgrade ourselves atomically)
if [[ -d "$STAGING_DIR/scripts" ]]; then
    log "Installing scripts (including potential self-upgrade)..."
    for script in "$STAGING_DIR/scripts/"*.sh; do
        if [[ -f "$script" ]]; then
            name=$(basename "$script")
            # Copy new version over old version (Linux allows this for running scripts)
            cp "$script" "$TARGET_BIN/$name"
            chmod 755 "$TARGET_BIN/$name"
            log "  Installed $name"
        fi
    done
fi

# Install dependencies from dependencies.json if present
install_dependencies() {
    local deps_file="$1"
    
    if [[ ! -f "$deps_file" ]]; then
        return 0
    fi
    
    log "Processing dependencies..."
    
    # Check if jq is available
    if ! command -v jq &>/dev/null; then
        log "WARNING: jq not installed, cannot process dependencies"
        return 0
    fi
    
    # Get list of adapters with apt dependencies
    local adapters
    adapters=$(jq -r '.linux | keys[]' "$deps_file" 2>/dev/null) || return 0
    
    for adapter in $adapters; do
        local adapter_binary="$TARGET_BIN/adapters/$adapter/garden-$adapter"
        
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
                        log "  Installing $pkg for $adapter ($reason)..."
                        if apt-get install -y -qq "$pkg" 2>&1; then
                            log "  Installed $pkg"
                            installed=true
                            break
                        fi
                    done
                    
                    if [[ "$installed" == "false" ]]; then
                        log "  WARNING: Failed to install $pkg_spec - $adapter may not work"
                    fi
                fi
            done
        fi
    done
}

# Install binaries (moss, rake, lantern, etc.) and adapters (cricket, etc.)
if [[ -d "$STAGING_DIR/bin" ]]; then
    log "Installing binaries..."
    # Copy recursively to preserve bin/adapters/ subdirectory
    cp -r "$STAGING_DIR/bin/"* "$TARGET_BIN/"
    # Set permissions on all files recursively
    find "$TARGET_BIN" -type f -exec chmod 755 {} \;
    # Log what was installed
    for binary in "$STAGING_DIR/bin/"*; do
        if [[ -f "$binary" ]]; then
            log "  Installed $(basename "$binary")"
        fi
    done
    # Log installed adapters (adapters are in subdirectories)
    if [[ -d "$STAGING_DIR/bin/adapters" ]]; then
        find "$STAGING_DIR/bin/adapters" -type f | while read -r adapter; do
            rel_path="${adapter#$STAGING_DIR/bin/}"
            log "  Installed $rel_path"
        done
    fi
fi

# Install dependencies from the staging area if present
if [[ -f "$STAGING_DIR/dependencies.json" ]]; then
    install_dependencies "$STAGING_DIR/dependencies.json"
fi

# Cleanup staging
rm -rf "$STAGING_DIR"
log "Installation complete"

exit 0
