#!/bin/sh
# Zen Garden v1 installer (macOS / Linux).
#
# Downloads the newest release's bundle for this platform, verifies the
# sha256 checksum, and installs moss + rake into ~/.local/bin (override
# with ZG_INSTALL_DIR).
#
#   curl -fsSL https://raw.githubusercontent.com/sylin-org/zen-garden/dev/installer/v1/install.sh | sh
#
# The repo slug can be overridden with ZG_REPO for forks.
set -eu

REPO="${ZG_REPO:-sylin-org/zen-garden}"
INSTALL_DIR="${ZG_INSTALL_DIR:-$HOME/.local/bin}"

os=$(uname -s)
arch=$(uname -m)
case "$os" in
    Linux) target="linux" ;;
    Darwin) target="macos" ;;
    *) echo "unsupported OS: $os (see installer/v1/install.ps1 for Windows)" >&2; exit 1 ;;
esac
case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
esac

bundle="zen-garden-${target}-${arch}.tar.gz"
base="https://github.com/${REPO}/releases/latest/download"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "fetching ${bundle}..."
curl -fsSL -o "$tmp/$bundle" "${base}/${bundle}"
curl -fsSL -o "$tmp/checksums.txt" "${base}/checksums.txt"

echo "verifying..."
want=$(grep " ${bundle}\$" "$tmp/checksums.txt" | cut -d' ' -f1)
[ -n "$want" ] || { echo "no checksum for ${bundle} — refusing" >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then
    got=$(sha256sum "$tmp/$bundle" | cut -d' ' -f1)
else
    got=$(shasum -a 256 "$tmp/$bundle" | cut -d' ' -f1)
fi
[ "$got" = "$want" ] || { echo "checksum MISMATCH: want ${want}, got ${got} — refusing" >&2; exit 1; }

tar xzf "$tmp/$bundle" -C "$tmp"
mkdir -p "$INSTALL_DIR"
mv "$tmp/moss" "$tmp/rake" "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/moss" "$INSTALL_DIR/rake"

echo
echo "installed: ${INSTALL_DIR}/moss, ${INSTALL_DIR}/rake"
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) echo "note: ${INSTALL_DIR} is not on your PATH — add it to use these directly" ;;
esac
echo "next: run 'rake observe' near a running moss, or start one: 'MOSS_RUNTIME=docker moss'"
