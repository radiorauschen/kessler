#!/bin/bash

set -euo pipefail

trap 'echo "make-app: abgebrochen in zeile $LINENO" >&2' ERR

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHARE=
if [[ "${1:-}" == "--share" ]]; then
  SHARE=1
  DEST="$ROOT/target/mac-share"
  rm -rf "$DEST"
  mkdir -p "$DEST"
else
  DEST="${1:-$ROOT}"
fi
APP="$DEST/kessler.app"
NAME="kessler"

if [[ "$(uname)" != "Darwin" ]]; then
  echo "This builds a macOS bundle; on $(uname) just run the binary directly." >&2
  exit 1
fi

if [[ ! -d "$DEST" ]]; then
  echo "No such folder: ${DEST}" >&2
  exit 1
fi

if [[ -n "$SHARE" ]]; then
  echo "Building the release binary for Apple silicon and for Intel..."
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  ( cd "$ROOT" && cargo build --release --target aarch64-apple-darwin \
               && cargo build --release --target x86_64-apple-darwin )
  BIN="$ROOT/target/mac-share/$NAME-universal"
  lipo -create \
    "$ROOT/target/aarch64-apple-darwin/release/$NAME" \
    "$ROOT/target/x86_64-apple-darwin/release/$NAME" \
    -output "$BIN"
else
  echo "Building the release binary..."
  ( cd "$ROOT" && cargo build --release )
  BIN="$ROOT/target/release/$NAME"
fi
[[ -x "$BIN" ]] || { echo "No binary at ${BIN}" >&2; exit 1; }

VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"

source "$ROOT/bundle/dmg-dress.sh"

echo "Assembling ${APP}..."
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/$NAME"

if command -v iconutil >/dev/null 2>&1; then
  if ! iconutil -c icns "$ROOT/bundle/icon.iconset" \
       -o "$APP/Contents/Resources/$NAME.icns" 2>/dev/null; then
    echo "  (iconutil could not build the icon -- using the generic one)"
    rm -f "$APP/Contents/Resources/$NAME.icns"
  fi
else
  echo "  (no iconutil -- the app will use the generic icon)"
fi

ICON_NAME=""
ICON_SOURCE=""
for candidate in "$ROOT/bundle"/*.icon "$ROOT"/*.icon; do
  [[ -d "$candidate" ]] || continue
  ICON_SOURCE="$candidate"
  break
done

if [[ -n "$ICON_SOURCE" ]]; then
  stem="$(basename "$ICON_SOURCE" .icon)"
  if xcrun --find actool >/dev/null 2>&1 &&
     xcrun actool "$ICON_SOURCE" \
       --compile "$APP/Contents/Resources" \
       --app-icon "$stem" \
       --include-all-app-icons \
       --enable-on-demand-resources NO \
       --target-device mac \
       --platform macosx \
       --minimum-deployment-target 26.0 \
       --development-region de \
       --output-partial-info-plist "$APP/Contents/Resources/.actool.plist" \
       --output-format human-readable-text >/dev/null 2>&1 &&
     [[ -f "$APP/Contents/Resources/Assets.car" ]]; then
    ICON_NAME="$stem"
    echo "  ($(basename "$ICON_SOURCE") compiled for macOS 26 and after)"
  else
    echo "  (actool could not compile $(basename "$ICON_SOURCE") -- macOS 26 will"
    echo "   use the flat icon instead. It needs Xcode 26 or later, not just the"
    echo "   command line tools.)"
  fi
  rm -f "$APP/Contents/Resources/.actool.plist"
fi

ICON_NAME_KEY=""
if [[ -n "$ICON_NAME" ]]; then
  ICON_NAME_KEY="    <key>CFBundleIconName</key>          <string>$ICON_NAME</string>"
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>              <string>kessler</string>
    <key>CFBundleDisplayName</key>       <string>kessler</string>
    <key>CFBundleIdentifier</key>        <string>de.windparksoft.kessler</string>
    <key>CFBundleExecutable</key>        <string>$NAME</string>
    <key>CFBundleIconFile</key>          <string>$NAME</string>
$ICON_NAME_KEY
    <key>CFBundlePackageType</key>       <string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key>           <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>    <string>11.0</string>
    <key>LSApplicationCategoryType</key> <string>public.app-category.music</string>
    <key>NSHighResolutionCapable</key>   <true/>
</dict>
</plist>
PLIST

cp "$ROOT/LICENSE" "$ROOT/LICENSE-drawing.md" "$APP/Contents/Resources/" ||
  echo "  (die lizenzen liessen sich nicht beilegen)"
if [[ -d "$ROOT/fonts" ]]; then
  echo "Copying the typefaces..."
  mkdir -p "$APP/Contents/Resources/fonts"
  if ! ( cd "$ROOT/fonts" &&
         WHICH=(./M\ PLUS\ 1\ Code ./Asap) &&
         { [[ -d ./Muto ]] && WHICH+=(./Muto); true; } &&
         find "${WHICH[@]}" \( -name '*.ttf' -o -name '*.otf' -o -name 'OFL.txt' -o -name 'FONTLOG.txt' -o -name 'LICENSE-muto.md' \) -print0 ) |
    while IFS= read -r -d '' face; do
      mkdir -p "$APP/Contents/Resources/fonts/$(dirname "$face")" || exit 1
      cp "$ROOT/fonts/$face" "$APP/Contents/Resources/fonts/$face" || exit 1
    done
  then
    echo "  (eine schrift liess sich nicht kopieren — die app nimmt die des systems)"
  fi
else
  echo "  (no fonts folder -- the app will offer the system's typefaces)"
fi

for needed in "Contents/Info.plist" "Contents/MacOS/$NAME"; do
  if [[ ! -e "$APP/$needed" ]]; then
    echo "make-app: $needed fehlt — die app ist unvollständig" >&2
    exit 1
  fi
done
if [[ ! -e "$APP/Contents/Resources/$NAME.icns" && ! -e "$APP/Contents/Resources/Assets.car" ]]; then
  echo "  (kein icon im bündel — macOS zeigt das allgemeine)"
fi

touch "$APP"

if [[ -n "$SHARE" ]]; then
  if find "$APP/Contents/Resources/fonts" -type f \( -name '*.otf' -o -name '*.ttf' \) 2>/dev/null |
       grep -v -e '/M PLUS 1 Code/' \
         -e '/fonts/Asap/' -e '/fonts/Muto/' >/dev/null; then
    echo "make-app: eine lizenzierte schrift ist im bündel — nicht weitergeben" >&2
    exit 1
  fi
  codesign --force --deep --sign - "$APP" >/dev/null 2>&1 \
    || echo "  (codesign fehlte — auf einem mac mit apple-chip startet sie so vielleicht nicht)"
  mkdir -p "$ROOT/dist"
  ZIP="$ROOT/dist/kessler-$VERSION-macos.zip"
  rm -f "$ZIP"
  ditto -c -k --keepParent "$APP" "$ZIP"
  DMG="$ROOT/dist/kessler-$VERSION-macos.dmg"
  STAGE="$DEST/dmg"
  rm -rf "$STAGE" "$DMG"
  mkdir -p "$STAGE"
  ditto "$APP" "$STAGE/kessler.app"
  ln -s /Applications "$STAGE/Applications"
  if ! dmg_dressed "$STAGE" "$DMG" "$DEST/dmg-work"; then
    echo "  (dmg ohne bild: swift, tiffutil oder der finder waren nicht zu haben)"
    rm -rf "$DMG" "$STAGE/.background"
    hdiutil create -volname "kessler $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null \
      || echo "  (hdiutil konnte keine dmg bauen — die zip ist da)"
  fi
  echo
  echo "Done: ${ZIP}"
  [[ -f "$DMG" ]] && echo "      ${DMG}"
  echo "To pass on. On the other Mac: unzip, drag kessler.app into Applications,"
  echo "then open it once with a right click → Öffnen (or System Settings →"
  echo "Privacy & Security → Open Anyway): it is not signed by a developer."
  exit 0
fi

echo
echo "Done: ${APP}"
echo "Double-click it, or drag it into your Applications folder."
