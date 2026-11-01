#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
DESKTOP_DIR="$ROOT_DIR/apps/desktop"
OUTPUT_DIR="$DESKTOP_DIR/dist"
ARCHITECTURE=x86_64
BUILD_DEB=0
BUILD_APPIMAGE=0
SKIP_BUILD=0
BINARY=""

usage() {
  cat <<'EOF'
Usage: scripts/build-linux-packages.sh [--deb] [--appimage] [options]

Build local release packages without dpkg-deb or mksquashfs.

Options:
  --deb                 Build a Debian binary package using ar and tar.
  --appimage            Build an AppImage using APPIMAGETOOL or PATH.
  --skip-build          Do not run cargo; require an existing release binary.
  --binary PATH         Use this Linux executable instead of target/release.
  --output-dir DIR      Package destination (default: apps/desktop/dist).
  --architecture ARCH   x86_64/amd64 or aarch64/arm64 (default: x86_64).
  -h, --help            Show this help.

With neither --deb nor --appimage, both package types are built.
EOF
}

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

while (($#)); do
  case "$1" in
    --deb) BUILD_DEB=1; shift ;;
    --appimage) BUILD_APPIMAGE=1; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --binary)
      (($# >= 2)) || fail "--binary requires a path"
      BINARY=$2
      shift 2
      ;;
    --output-dir)
      (($# >= 2)) || fail "--output-dir requires a directory"
      OUTPUT_DIR=$2
      shift 2
      ;;
    --architecture)
      (($# >= 2)) || fail "--architecture requires a value"
      ARCHITECTURE=$2
      shift 2
      ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown option: $1" ;;
  esac
done

if (( ! BUILD_DEB && ! BUILD_APPIMAGE )); then
  BUILD_DEB=1
  BUILD_APPIMAGE=1
fi

case "$ARCHITECTURE" in
  x86_64|amd64)
    DEB_ARCHITECTURE=amd64
    APPIMAGE_ARCHITECTURE=x86_64
    ;;
  aarch64|arm64)
    DEB_ARCHITECTURE=arm64
    APPIMAGE_ARCHITECTURE=aarch64
    ;;
  *) fail "unsupported architecture: $ARCHITECTURE" ;;
esac

VERSION=$(awk -F'"' '/^version = "/ { print $2; exit }' "$DESKTOP_DIR/linux/Cargo.toml")
[[ -n "$VERSION" ]] || fail "cannot read version from linux/Cargo.toml"
[[ -s "$ROOT_DIR/LICENSE" ]] || fail "missing root LICENSE"
[[ -s "$ROOT_DIR/THIRD_PARTY_NOTICES.md" ]] || fail "missing root THIRD_PARTY_NOTICES.md"

if [[ -z "$BINARY" ]]; then
  BINARY="$DESKTOP_DIR/target/release/aimemorandum"
fi
if (( ! SKIP_BUILD )) && [[ "$BINARY" == "$DESKTOP_DIR/target/release/aimemorandum" ]]; then
  cargo build --release --manifest-path "$DESKTOP_DIR/Cargo.toml" -p aimemorandum-linux
fi
[[ -f "$BINARY" ]] || fail "Linux executable not found: $BINARY"

mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(CDPATH= cd -- "$OUTPUT_DIR" && pwd)"
WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/aimemorandum-packaging.XXXXXX")
trap 'rm -rf "$WORK_DIR"' EXIT

build_deb() {
  local stage="$WORK_DIR/deb"
  local package="$OUTPUT_DIR/aimemorandum_${VERSION}_${DEB_ARCHITECTURE}.deb"

  mkdir -p     "$stage/data/usr/bin"     "$stage/data/usr/share/applications"     "$stage/data/usr/share/icons/hicolor/scalable/apps"     "$stage/data/usr/share/doc/aimemorandum"     "$stage/control"
  cp "$BINARY" "$stage/data/usr/bin/aimemorandum"
  chmod 0755 "$stage/data/usr/bin/aimemorandum"
  cp "$DESKTOP_DIR/linux/packaging/aimemorandum.desktop"     "$stage/data/usr/share/applications/aimemorandum.desktop"
  cp "$DESKTOP_DIR/linux/packaging/aimemorandum.svg"     "$stage/data/usr/share/icons/hicolor/scalable/apps/aimemorandum.svg"
  cp "$ROOT_DIR/LICENSE" "$ROOT_DIR/THIRD_PARTY_NOTICES.md"     "$stage/data/usr/share/doc/aimemorandum/"

  cat >"$stage/control/control" <<EOF
Package: aimemorandum
Version: $VERSION
Architecture: $DEB_ARCHITECTURE
Section: utils
Priority: optional
Maintainer: AIMemorandum contributors
Description: AI-assisted memorandum desktop application
 Local desktop build. Runtime library requirements are those of the
 locally built executable and have not been validated on a clean system.
EOF

  tar --numeric-owner --owner=0 --group=0 -czf "$stage/control.tar.gz"     -C "$stage/control" ./control
  tar --numeric-owner --owner=0 --group=0 -czf "$stage/data.tar.gz"     -C "$stage/data" .
  printf '2.0\n' >"$stage/debian-binary"
  rm -f "$package"
  (cd "$stage" && ar rcs "$package" debian-binary control.tar.gz data.tar.gz)
  printf 'Built %s\n' "$package"
}

build_appimage() {
  local appdir="$WORK_DIR/appimage/AIMemorandum.AppDir"
  local package="$OUTPUT_DIR/AIMemorandum-${VERSION}-${APPIMAGE_ARCHITECTURE}.AppImage"
  local tool

  if [[ -n "${APPIMAGETOOL:-}" ]]; then
    tool=$APPIMAGETOOL
  else
    tool=$(command -v appimagetool) || fail "set APPIMAGETOOL or put appimagetool on PATH"
  fi

  mkdir -p "$appdir/usr/bin" "$appdir/usr/share/doc/aimemorandum"
  cp "$BINARY" "$appdir/usr/bin/aimemorandum"
  chmod 0755 "$appdir/usr/bin/aimemorandum"
  cp "$DESKTOP_DIR/linux/packaging/aimemorandum.desktop" "$appdir/aimemorandum.desktop"
  cp "$DESKTOP_DIR/linux/packaging/aimemorandum.svg" "$appdir/aimemorandum.svg"
  cp "$DESKTOP_DIR/linux/packaging/aimemorandum.png" "$appdir/aimemorandum.png"
  cp "$ROOT_DIR/LICENSE" "$ROOT_DIR/THIRD_PARTY_NOTICES.md"     "$appdir/usr/share/doc/aimemorandum/"
  cat >"$appdir/AppRun" <<'EOF'
#!/bin/sh
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$HERE/usr/bin/aimemorandum" "$@"
EOF
  chmod 0755 "$appdir/AppRun"

  rm -f "$package"
  ARCH="$APPIMAGE_ARCHITECTURE" "$tool" "$appdir" "$package"
  [[ -s "$package" ]] || fail "appimagetool did not create $package"
  chmod 0755 "$package"
  printf 'Built %s\n' "$package"
}

if (( BUILD_DEB )); then
  build_deb
fi
if (( BUILD_APPIMAGE )); then
  build_appimage
fi
