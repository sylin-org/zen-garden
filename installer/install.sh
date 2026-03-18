#!/bin/bash
# install.sh — One-liner Zen Garden installer for Linux
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/sylin-org/zen-garden/dev/installer/install.sh | sudo bash
#   curl -fsSL .../install.sh | sudo bash -s -- --provision
#   curl -fsSL .../install.sh | sudo bash -s -- --dry-run
#
# All flags are forwarded to `garden-moss install`.

set -euo pipefail

REPO="sylin-org/zen-garden"

# ── Platform detection ───────────────────────────────────────────────

ARCH=$(uname -m)
case "$ARCH" in
    x86_64|amd64)  PLATFORM="linux-x64" ;;
    i686|i386)     PLATFORM="linux-x86" ;;
    *)
        echo "Unsupported architecture: $ARCH"
        echo "Zen Garden supports: x86_64, i686"
        exit 1
        ;;
esac

# ── Privilege check ──────────────────────────────────────────────────

if [[ $EUID -ne 0 ]]; then
    echo "This installer requires root. Run with sudo:"
    echo "  curl -fsSL https://raw.githubusercontent.com/$REPO/dev/installer/install.sh | sudo bash"
    exit 1
fi

# ── Fetch latest release ────────────────────────────────────────────

echo ""
echo "  Zen Garden Installer"
echo ""

echo "Fetching latest release from GitHub..."
RELEASE_JSON=$(curl -fsSL \
    -H "Accept: application/vnd.github+json" \
    -H "User-Agent: zen-garden-installer" \
    "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null) || {
    echo "Could not reach GitHub API. For offline install, download manually:"
    echo "  https://github.com/$REPO/releases/latest"
    exit 1
}

VERSION=$(echo "$RELEASE_JSON" | grep -o '"tag_name":"[^"]*"' | head -1 | cut -d'"' -f4)
echo "  Latest version: ${VERSION:-unknown}"

# ── Find matching assets ────────────────────────────────────────────

# Find garden-moss binary URL (standalone binary asset)
MOSS_URL=$(echo "$RELEASE_JSON" | grep -o "\"browser_download_url\":\"[^\"]*garden-moss-${PLATFORM}[^\"]*\"" | head -1 | cut -d'"' -f4)

# Find package URL
PKG_URL=$(echo "$RELEASE_JSON" | grep -o "\"browser_download_url\":\"[^\"]*zen-garden-[^\"]*-${PLATFORM}\.tar\.gz\"" | head -1 | cut -d'"' -f4)

if [[ -z "$PKG_URL" ]]; then
    echo "No package found for platform: $PLATFORM"
    echo "Available assets at: https://github.com/$REPO/releases/latest"
    exit 1
fi

# ── Download ─────────────────────────────────────────────────────────

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

# If standalone moss binary is available, download it; otherwise extract from package
if [[ -n "$MOSS_URL" ]]; then
    echo "Downloading garden-moss..."
    curl -fSL --progress-bar -o "$TMPDIR/garden-moss" "$MOSS_URL"
    chmod +x "$TMPDIR/garden-moss"
else
    echo "No standalone binary found, will extract from package."
fi

PKG_NAME=$(basename "$PKG_URL")
echo "Downloading $PKG_NAME..."
curl -fSL --progress-bar -o "$TMPDIR/$PKG_NAME" "$PKG_URL"

# If no standalone binary, extract garden-moss from the package
if [[ ! -x "$TMPDIR/garden-moss" ]]; then
    echo "Extracting garden-moss from package..."
    tar -xzf "$TMPDIR/$PKG_NAME" -C "$TMPDIR" --wildcards '*/bin/garden-moss' 2>/dev/null || true
    EXTRACTED=$(find "$TMPDIR" -name garden-moss -type f | head -1)
    if [[ -n "$EXTRACTED" ]]; then
        cp "$EXTRACTED" "$TMPDIR/garden-moss"
        chmod +x "$TMPDIR/garden-moss"
    else
        echo "Could not find garden-moss in package. Download manually."
        exit 1
    fi
fi

# ── Run install ──────────────────────────────────────────────────────

echo ""
echo "Running garden-moss install..."
echo ""

# Auto-accept prompts (non-interactive context) + forward user flags
"$TMPDIR/garden-moss" install --yes "$@"
