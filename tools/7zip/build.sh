#!/bin/zsh
# Builds the 7-Zip command-line helper (7zz) that Zipling runs, unmodified, as a
# separate process to read and write ZIP, 7z, RAR, DMG, ISO and ~60 other formats.
#
# The source is the official 7-Zip release, pinned by version and SHA-256. It is
# downloaded once into tools/7zip/cache (git-ignored) and built for the app's
# deployment target. Output: tools/7zip/bin/7zz
#
# 7-Zip is GNU LGPL (with the unRAR restriction on its RAR decoder); see
# tools/7zip/README.md for what Zipling ships and how to get the exact source.
set -euo pipefail

VERSION=26.05
TAG=2605
SHA256=0b6080b8c3e7a579161d2d495224c0e51c467576cb4c7048b0ab485ef44cc22e
URL="https://github.com/ip7z/7zip/releases/download/${VERSION}/7z${TAG}-src.tar.xz"

HERE="${0:A:h}"
CACHE="$HERE/cache"
OUT="$HERE/bin/7zz"
STAMP="$HERE/bin/.version"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-15.0}"

if [[ -x "$OUT" && "$(cat "$STAMP" 2>/dev/null)" == "$VERSION-$MACOSX_DEPLOYMENT_TARGET" ]]; then
  exit 0
fi

mkdir -p "$CACHE" "$HERE/bin"
TARBALL="$CACHE/7z${TAG}-src.tar.xz"
if [[ ! -f "$TARBALL" ]] || [[ "$(shasum -a 256 "$TARBALL" | cut -d' ' -f1)" != "$SHA256" ]]; then
  echo "Downloading 7-Zip $VERSION source"
  curl -fsSL "$URL" -o "$TARBALL.part"
  mv "$TARBALL.part" "$TARBALL"
fi
if [[ "$(shasum -a 256 "$TARBALL" | cut -d' ' -f1)" != "$SHA256" ]]; then
  echo "error: 7-Zip source checksum mismatch" >&2
  rm -f "$TARBALL"
  exit 1
fi

SRC="$CACHE/src-$VERSION"
rm -rf "$SRC"
mkdir -p "$SRC"
tar -xJf "$TARBALL" -C "$SRC"

echo "Building 7zz $VERSION for macOS $MACOSX_DEPLOYMENT_TARGET (arm64)"
cd "$SRC/CPP/7zip/Bundles/Alone2"
make -s -j"$(sysctl -n hw.ncpu)" -f ../../cmpl_mac_arm64.mak \
  CFLAGS_BASE_LIST="-c -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET" \
  LDFLAGS_STATIC_3="-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET" >/dev/null
cp b/m_arm64/7zz "$OUT"
strip -x "$OUT"
echo "$VERSION-$MACOSX_DEPLOYMENT_TARGET" > "$STAMP"
echo "Built $OUT"
