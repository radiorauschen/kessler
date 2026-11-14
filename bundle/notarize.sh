#!/bin/bash

set -euo pipefail
trap 'echo "notarize: abgebrochen in zeile $LINENO" >&2' ERR

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAME="kessler"
APP="$ROOT/target/mac-share/$NAME.app"
IDENTITY="${KESSLER_SIGN_IDENTITY:-}"
PROFILE="${KESSLER_NOTARY_PROFILE:-kessler-notary}"
ENTITLEMENTS="$ROOT/bundle/kessler.entitlements"

say() { echo "$@"; }
fail() { echo "notarize: $*" >&2; exit 1; }

BUILD=
for arg in "$@"; do
  case "$arg" in
    --build) BUILD=1 ;;
    -h|--help) sed -n '2,45p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) fail "unbekanntes argument: $arg (nur --build)" ;;
  esac
done

[[ "$(uname)" == "Darwin" ]] || fail "das geht nur auf einem mac (hier: $(uname))"

for tool in codesign ditto hdiutil spctl security plutil xattr file xcrun; do
  command -v "$tool" >/dev/null 2>&1 || fail "$tool fehlt"
done
xcrun --find notarytool >/dev/null 2>&1 ||
  fail "notarytool fehlt — Xcode 13 oder neuer bzw. die command line tools installieren (xcode-select --install)"
xcrun --find stapler >/dev/null 2>&1 || fail "stapler fehlt (kommt mit Xcode)"

[[ -n "$IDENTITY" ]] ||
  fail "KESSLER_SIGN_IDENTITY ist nicht gesetzt — siehe den kopf dieses scripts"
[[ "$IDENTITY" == "Developer ID Application:"* ]] ||
  fail "„$IDENTITY\" ist keine Developer-ID-Application-identität (nur die wird notarisiert)"
security find-identity -v -p codesigning | grep -qF "\"$IDENTITY\"" ||
  fail "die identität „$IDENTITY\" ist nicht (gültig) im schlüsselbund — security find-identity -v -p codesigning"

xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null 2>&1 ||
  fail "das notar-profil „$PROFILE\" geht nicht — xcrun notarytool store-credentials $PROFILE … (siehe kopf), oder kein netz"

[[ -f "$ENTITLEMENTS" ]] || fail "$ENTITLEMENTS fehlt"
plutil -lint "$ENTITLEMENTS" >/dev/null || fail "$ENTITLEMENTS ist kein gültiges plist"

if [[ -n "$BUILD" ]]; then
  "$ROOT/bundle/make-app.sh" --share
fi
[[ -x "$APP/Contents/MacOS/$NAME" ]] ||
  fail "keine app unter $APP — erst ./bundle/make-app.sh --share (oder --build)"

VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
PLIST_VERSION="$(plutil -extract CFBundleShortVersionString raw -o - "$APP/Contents/Info.plist")"
[[ "$VERSION" == "$PLIST_VERSION" ]] ||
  fail "die app ist $PLIST_VERSION, Cargo.toml sagt $VERSION — neu bauen (--build)"

FONTS="$APP/Contents/Resources/fonts"
if find "$FONTS" -type f \( -iname '*.otf' -o -iname '*.ttf' -o -iname '*.woff*' \) 2>/dev/null |
     grep -v -e '/fonts/M PLUS 1 Code/' -e '/fonts/Asap/' -e '/fonts/Muto/' >/dev/null; then
  fail "eine lizenzierte schrift ist im bündel — nicht weitergeben"
fi

WORK="$(mktemp -d "${TMPDIR:-/tmp}/kessler-notarize.XXXXXX")"
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

notarize() {
  local file="$1" answer="$WORK/answer.plist" id status
  say "Submitting $(basename "$file") to Apple (this can take minutes)..."
  xcrun notarytool submit "$file" --keychain-profile "$PROFILE" --wait \
    --output-format plist > "$answer" || true
  id="$(plutil -extract id raw -o - "$answer" 2>/dev/null || true)"
  status="$(plutil -extract status raw -o - "$answer" 2>/dev/null || true)"
  if [[ "$status" != "Accepted" ]]; then
    echo "notarize: apple sagt „${status:-keine antwort}\" zu $(basename "$file")" >&2
    if [[ -n "$id" ]]; then
      echo "--- notarytool log $id ---" >&2
      xcrun notarytool log "$id" --keychain-profile "$PROFILE" >&2 || true
    else
      cat "$answer" >&2 || true
    fi
    exit 1
  fi
  say "  accepted ($id)"
}

say "Signing $APP as"
say "  $IDENTITY"

if ! plutil -extract NSAppleEventsUsageDescription raw -o - "$APP/Contents/Info.plist" >/dev/null 2>&1; then
  plutil -insert NSAppleEventsUsageDescription -string \
    "kessler hands music you import to Music and asks Music which tracks it already knows." \
    "$APP/Contents/Info.plist"
fi

xattr -cr "$APP"

while IFS= read -r -d '' file; do
  [[ "$file" == "$APP/Contents/MacOS/$NAME" ]] && continue
  if file -b "$file" | grep -q 'Mach-O'; then
    say "  inner: ${file#"$APP"/}"
    codesign --force --timestamp --options runtime --sign "$IDENTITY" "$file"
  fi
done < <(find "$APP/Contents" -type f -print0)

codesign --force --timestamp --options runtime \
  --entitlements "$ENTITLEMENTS" --sign "$IDENTITY" "$APP/Contents/MacOS/$NAME"
codesign --force --timestamp --options runtime \
  --entitlements "$ENTITLEMENTS" --sign "$IDENTITY" "$APP"

codesign --verify --strict --deep --verbose=2 "$APP" ||
  fail "die signatur hält der prüfung nicht stand"

ZIP_IN="$WORK/$NAME-for-notary.zip"
ditto -c -k --keepParent "$APP" "$ZIP_IN"
notarize "$ZIP_IN"

xcrun stapler staple "$APP"
xcrun stapler validate "$APP"

mkdir -p "$ROOT/dist"
ZIP="$ROOT/dist/$NAME-$VERSION-macos.zip"
DMG="$ROOT/dist/$NAME-$VERSION-macos.dmg"
rm -f "$ZIP" "$DMG"

ditto -c -k --keepParent "$APP" "$ZIP"

STAGE="$WORK/dmg"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/$NAME.app"
ln -s /Applications "$STAGE/Applications"
source "$ROOT/bundle/dmg-dress.sh"
if ! dmg_dressed "$STAGE" "$DMG" "$WORK/dmg-work"; then
  say "  (dmg ohne bild: swift, tiffutil oder der finder waren nicht zu haben)"
  rm -rf "$DMG" "$STAGE/.background"
  hdiutil create -volname "$NAME $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null ||
    fail "hdiutil konnte keine dmg bauen"
fi

codesign --force --timestamp --sign "$IDENTITY" "$DMG"
codesign --verify --verbose=2 "$DMG"
notarize "$DMG"
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"

say
say "Gatekeeper, the app:"
spctl --assess --type execute -vv "$APP"
say "Gatekeeper, the disk image:"
spctl --assess --type open --context context:primary-signature -vv "$DMG"

say
say "Done: $ZIP"
say "      $DMG"
say "Signed, notarized and stapled: opens on any Mac without a right click."
