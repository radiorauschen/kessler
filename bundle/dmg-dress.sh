dmg_dressed() {
  local stage="$1" out="$2" work="$3"
  local volume="kessler $VERSION"
  local commit
  commit="$(git -C "$ROOT" rev-parse --short=7 HEAD 2>/dev/null || true)"
  rm -rf "$work" && mkdir -p "$work" || return 1
  command -v swift >/dev/null || { echo "  dmg: swift fehlt (xcode-select --install)"; return 1; }
  command -v tiffutil >/dev/null || { echo "  dmg: tiffutil fehlt"; return 1; }
  echo "  dmg: version ins bild…"
  swift "$ROOT/bundle/dmg-stamp.swift" "$ROOT/bundle/dmg/background.png" "$work/bg.png" 1 \
    "$ROOT/fonts/Asap/Asap-Variable.ttf" "$VERSION" "$commit" || { echo "  dmg: dmg-stamp.swift ging nicht"; return 1; }
  swift "$ROOT/bundle/dmg-stamp.swift" "$ROOT/bundle/dmg/background@2x.png" "$work/bg@2x.png" 2 \
    "$ROOT/fonts/Asap/Asap-Variable.ttf" "$VERSION" "$commit" || { echo "  dmg: dmg-stamp.swift ging nicht (@2x)"; return 1; }
  tiffutil -cathidpicheck "$work/bg.png" "$work/bg@2x.png" -out "$work/background.tiff" >/dev/null \
    || { echo "  dmg: tiffutil ging nicht"; return 1; }
  mkdir -p "$stage/.background" && cp "$work/background.tiff" "$stage/.background/background.tiff"
  hdiutil create -volname "$volume" -srcfolder "$stage" -ov -format UDRW "$work/rw.dmg" >/dev/null \
    || { echo "  dmg: beschreibbares image ging nicht"; return 1; }
  if [[ -d "/Volumes/$volume" ]]; then
    echo "  dmg: „$volume“ ist noch geöffnet — wird ausgeworfen…"
    hdiutil detach "/Volumes/$volume" -quiet || hdiutil detach "/Volumes/$volume" -force -quiet || true
  fi
  local mounted
  mounted="$(hdiutil attach -readwrite -noverify -noautoopen "$work/rw.dmg" | awk -F'\t' '/\/Volumes\// {print $NF; exit}')"
  [[ -n "$mounted" ]] || { echo "  dmg: image liess sich nicht einhängen"; return 1; }
  local disk="${mounted##*/}"
  echo "  dmg: der finder richtet das fenster ein…"
  osascript >/dev/null <<APPLESCRIPT
tell application "Finder"
  tell disk "$disk"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {160, 120, 840, 848}
    set view to the icon view options of container window
    set arrangement of view to not arranged
    set icon size of view to 80
    set text size of view to 12
    set background picture of view to file ".background:background.tiff"
    set position of item "kessler.app" of container window to {410, 133}
    set position of item "Applications" of container window to {184, 220}
    close
    open
    update without registering applications
    delay 1
    close
  end tell
end tell
APPLESCRIPT
  local dressed=$?
  sync
  hdiutil detach "$mounted" -quiet || hdiutil detach "$mounted" -force -quiet
  [[ $dressed -eq 0 ]] || { echo "  dmg: der finder liess sich nicht steuern (systemeinstellungen → datenschutz → automation: terminal → finder erlauben)"; return 1; }
  rm -f "$out"
  hdiutil convert "$work/rw.dmg" -format UDZO -o "$out" >/dev/null || return 1
  rm -rf "$work"
}
