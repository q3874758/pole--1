#!/bin/bash
set -e

echo "===================================="
echo "PoLE Linux DEB Packaging Script"
echo "===================================="

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
RELEASE_DIR="$ROOT_DIR/target/release"
PKG_DIR="$DIST_DIR/packages"
DEB_DIR="$PKG_DIR/deb-build"
DEB_OUT="$PKG_DIR/pole-node_0.1.0_amd64.deb"

mkdir -p "$PKG_DIR"
mkdir -p "$DEB_DIR/DEBIAN"
mkdir -p "$DEB_DIR/etc/systemd/system"
mkdir -p "$DEB_DIR/etc/pole"
mkdir -p "$DEB_DIR/var/lib/pole"
mkdir -p "$DEB_DIR/var/log/pole"
mkdir -p "$DEB_DIR/opt/pole"

echo "[1/3] Copying binaries..."
cp "$RELEASE_DIR/pole-node" "$DEB_DIR/opt/pole/" 2>/dev/null || true
cp "$RELEASE_DIR/pole-client" "$DEB_DIR/opt/pole/" 2>/dev/null || true

echo "[2/3] Copying systemd unit..."
cp "$ROOT_DIR/packaging/linux/deb/pole-node.service" "$DEB_DIR/etc/systemd/system/"

echo "[3/3] Building DEB package..."
cp "$ROOT_DIR/packaging/linux/deb/control" "$DEB_DIR/DEBIAN/"
cp "$ROOT_DIR/packaging/linux/deb/postinst" "$DEB_DIR/DEBIAN/postinst"
cp "$ROOT_DIR/packaging/linux/deb/prerm" "$DEB_DIR/DEBIAN/prerm"
chmod 755 "$DEB_DIR/DEBIAN/postinst" "$DEB_DIR/DEBIAN/prerm"

dpkg-deb --build "$DEB_DIR" "$DEB_OUT"

echo "===================================="
echo "DEB created: $DEB_OUT"
echo "===================================="
