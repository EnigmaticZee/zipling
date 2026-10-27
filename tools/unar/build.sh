#!/bin/zsh
# Builds The Unarchiver's command-line tools (lsar and unar) that Zipling runs,
# unmodified, as separate processes: classic Mac formats (StuffIt, BinHex,
# MacBinary, Compact Pro…), the long tail 7-Zip doesn't read, and detection of
# legacy file-name encodings (Shift-JIS, GB18030, Big5, CP437…).
#
# The source is MacPaw's XADMaster release and its UniversalDetector, pinned by
# version and SHA-256 (the same pair Homebrew builds). It is downloaded once into
# tools/unar/cache (git-ignored). Output: tools/unar/bin/lsar and tools/unar/bin/unar
#
# XADMaster is GNU LGPL 2.1 or later; see tools/unar/README.md.
set -euo pipefail

VERSION=1.10.8
SHA256=652953d7988b3c33f4f52b61c357afd1a7c2fc170e5e6e2219f4432b0c4cd39f
URL="https://github.com/MacPaw/XADMaster/archive/refs/tags/v${VERSION}.tar.gz"
DETECTOR_VERSION=1.1
DETECTOR_SHA256=8e8532111d0163628eb828a60d67b53133afad3f710b1967e69d3b8eee28a811
DETECTOR_URL="https://github.com/MacPaw/universal-detector/archive/refs/tags/${DETECTOR_VERSION}.tar.gz"

HERE="${0:A:h}"
CACHE="$HERE/cache"
STAMP="$HERE/bin/.version"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-15.0}"

if [[ -x "$HERE/bin/lsar" && -x "$HERE/bin/unar" && "$(cat "$STAMP" 2>/dev/null)" == "$VERSION-$MACOSX_DEPLOYMENT_TARGET" ]]; then
  exit 0
fi

fetch() {  # url, sha256, file
  if [[ ! -f "$3" ]] || [[ "$(shasum -a 256 "$3" | cut -d' ' -f1)" != "$2" ]]; then
    echo "Downloading $(basename "$3")"
    curl -fsSL "$1" -o "$3.part"
    mv "$3.part" "$3"
  fi
  if [[ "$(shasum -a 256 "$3" | cut -d' ' -f1)" != "$2" ]]; then
    echo "error: checksum mismatch for $(basename "$3")" >&2
    rm -f "$3"
    exit 1
  fi
}

mkdir -p "$CACHE" "$HERE/bin"
fetch "$URL" "$SHA256" "$CACHE/XADMaster-$VERSION.tar.gz"
fetch "$DETECTOR_URL" "$DETECTOR_SHA256" "$CACHE/universal-detector-$DETECTOR_VERSION.tar.gz"

# XADMaster's project expects UniversalDetector next to it.
SRC="$CACHE/src-$VERSION"
rm -rf "$SRC"
mkdir -p "$SRC/XADMaster" "$SRC/UniversalDetector"
tar -xzf "$CACHE/XADMaster-$VERSION.tar.gz" -C "$SRC/XADMaster" --strip-components 1
tar -xzf "$CACHE/universal-detector-$DETECTOR_VERSION.tar.gz" -C "$SRC/UniversalDetector" --strip-components 1
# Link libc++ rather than the long-gone libstdc++.
sed -i '' 's/libstdc++.6.dylib/libc++.1.dylib/g' "$SRC/XADMaster/XADMaster.xcodeproj/project.pbxproj"

echo "Building lsar and unar $VERSION for macOS $MACOSX_DEPLOYMENT_TARGET (arm64)"
BUILD="$SRC/build"
for target in XADMaster unar lsar; do
  xcodebuild -quiet -target "$target" -project "$SRC/XADMaster/XADMaster.xcodeproj" -configuration Release \
    SYMROOT="$BUILD" MACOSX_DEPLOYMENT_TARGET="$MACOSX_DEPLOYMENT_TARGET" ARCHS=arm64 ONLY_ACTIVE_ARCH=YES
done
for tool in lsar unar; do
  cp "$BUILD/Release/$tool" "$HERE/bin/$tool"
  strip -x "$HERE/bin/$tool"
done
echo "$VERSION-$MACOSX_DEPLOYMENT_TARGET" > "$STAMP"
echo "Built $HERE/bin/lsar and $HERE/bin/unar"
