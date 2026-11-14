#!/bin/bash

set -euo pipefail

trap 'echo "make-linux-package: abgebrochen in zeile $LINENO" >&2' ERR

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
case "${1:-arm64}" in
  arm64|aarch64)
    TARGET=aarch64-unknown-linux-gnu; ARCH=arm64; DEBARCH=arm64
    TRIPLE=aarch64-linux-gnu; ARCHIVE=http://ports.ubuntu.com/ubuntu-ports ;;
  x86_64|amd64)
    TARGET=x86_64-unknown-linux-gnu; ARCH=x86_64; DEBARCH=amd64
    TRIPLE=x86_64-linux-gnu; ARCHIVE=http://archive.ubuntu.com/ubuntu ;;
  *) echo "make-linux-package: arm64 or x86_64, not ${1}" >&2; exit 1 ;;
esac
GLIBC=2.31
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
NAME="kessler-$VERSION-linux-$ARCH"
FOLDER="kessler-linux-$ARCH"
WORK="$ROOT/target/linux-package/$ARCH"
SYSROOT="$WORK/sysroot"

rustup target add "$TARGET" >/dev/null

USER_BIN="$(python3 -m site --user-base)/bin"
export PATH="$USER_BIN:$HOME/.local/bin:$PATH"
pip_install() {
  python3 -m pip install --user --quiet "$@" 2>/dev/null \
    || python3 -m pip install --user --quiet --break-system-packages "$@"
}

if ! command -v cargo-zigbuild >/dev/null; then
  echo "Installing zig and cargo-zigbuild..."
  pip_install cargo-zigbuild
fi

if ! command -v pkg-config >/dev/null; then
  echo "Installing pkg-config..."
  pip_install pkgconf
fi

if [[ ! -e "$SYSROOT/usr/lib/$TRIPLE/libasound.so" ]]; then
  echo "Fetching ALSA for $ARCH..."
  mkdir -p "$SYSROOT" "$WORK/debs"
  base="$ARCHIVE/pool/main/a/alsa-lib"
  for deb in "libasound2_1.1.3-5_$DEBARCH.deb" "libasound2-dev_1.1.3-5_$DEBARCH.deb"; do
    curl -sSfL -o "$WORK/debs/$deb" "$base/$deb"
    dpkg-deb -x "$WORK/debs/$deb" "$SYSROOT" 2>/dev/null \
      || (cd "$SYSROOT" && ar p "$WORK/debs/$deb" data.tar.xz | tar -xJ)
  done
fi

echo "Building kessler $VERSION for $TARGET (glibc $GLIBC)..."
( cd "$ROOT" && \
  PKG_CONFIG_ALLOW_CROSS=1 \
  PKG_CONFIG_SYSROOT_DIR="$SYSROOT" \
  PKG_CONFIG_LIBDIR="$SYSROOT/usr/lib/$TRIPLE/pkgconfig" \
  cargo zigbuild --release --target "$TARGET.$GLIBC" )

echo "Packing $NAME..."
STAGE="$WORK/$FOLDER"
rm -rf "$STAGE"
mkdir -p "$STAGE/fonts" "$STAGE/icon.iconset" "$STAGE/icon-gnome" "$STAGE/icon-kde"
cp "$ROOT/target/$TARGET/release/kessler" "$STAGE/kessler"
for face in "$ROOT"/fonts/M\ PLUS\ 1\ Code/ \
            "$ROOT"/fonts/Asap/ "$ROOT"/fonts/Muto/; do
  if [[ -d "$face" ]]; then cp -R "$face" "$STAGE/fonts/"; fi
done
cp "$ROOT"/bundle/icon.iconset/*.png "$STAGE/icon.iconset/"
cp "$ROOT"/bundle/icon-gnome/*.png "$STAGE/icon-gnome/"
cp "$ROOT"/bundle/icon-kde/*.png "$STAGE/icon-kde/"
cp "$ROOT/bundle/install-linux.sh" "$STAGE/install.sh"
cp "$ROOT/CHANGELOG.md" "$STAGE/"
cp "$ROOT/LICENSE" "$ROOT/LICENSE-drawing.md" "$STAGE/"

mkdir -p "$ROOT/dist"
tar -C "$WORK" -czf "$ROOT/dist/$NAME.tar.gz" "$FOLDER"
echo "Done: dist/$NAME.tar.gz"
