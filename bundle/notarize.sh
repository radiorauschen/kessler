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
    --no-upload) UPLOAD=0 ;;
    -h|--help)
      cat <<'USAGE'
./bundle/notarize.sh             sign, notarize and staple target/mac-share/kessler.app
./bundle/notarize.sh --build     build it first (make-app.sh --share)
./bundle/notarize.sh --no-upload leave the dmg on this mac only

needs KESSLER_SIGN_IDENTITY ("Developer ID Application: …") and a notary
profile (xcrun notarytool store-credentials kessler-notary …;
KESSLER_NOTARY_PROFILE for another name). the dmg then goes to the release
„mac" of radiorauschen/kessler as kessler-macos.dmg (gh, logged in):
https://github.com/radiorauschen/kessler/releases/download/mac/kessler-macos.dmg
USAGE
      exit 0 ;;
    *) fail "unbekanntes argument: $arg (nur --build, --no-upload)" ;;
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

if [[ "${UPLOAD:-1}" == "1" ]]; then
  REPO="${KESSLER_RELEASE_REPO:-radiorauschen/kessler}"
  TAG="mac"
  say
  if ! command -v gh >/dev/null 2>&1; then
    say "Nicht hochgeladen: gh fehlt (brew install gh, dann gh auth login)."
  elif ! gh auth status >/dev/null 2>&1; then
    say "Nicht hochgeladen: gh ist nicht angemeldet (gh auth login)."
  else
    FIXED="$WORK/$NAME-macos.dmg"
    cp "$DMG" "$FIXED"
    NOTES="$NAME $VERSION für den mac, signiert und notarisiert. dieser link zeigt immer auf die neueste notarisierte fassung."
    if ! gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
      gh release create "$TAG" --repo "$REPO" --prerelease --title "$NAME für den mac" --notes "$NOTES" ||
        fail "das release „$TAG\" liess sich nicht anlegen"
    else
      gh release edit "$TAG" --repo "$REPO" --notes "$NOTES" >/dev/null
    fi
    gh release upload "$TAG" "$FIXED" --repo "$REPO" --clobber ||
      fail "die dmg liess sich nicht hochladen"
    say "Hochgeladen: https://github.com/$REPO/releases/download/$TAG/$NAME-macos.dmg"
  fi
fi
