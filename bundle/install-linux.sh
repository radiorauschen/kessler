#!/bin/bash

set -euo pipefail

trap 'echo "install-linux: abgebrochen in zeile $LINENO" >&2' ERR

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
if [[ -x "$HERE/kessler" && -d "$HERE/icon.iconset" ]]; then
  PACKAGED=1
  BIN="$HERE/kessler"
  ICONS="$HERE/icon.iconset"
else
  PACKAGED=
  ROOT="$(cd "$HERE/.." && pwd)"
  BIN="$ROOT/target/release/kessler"
  ICONS="$ROOT/bundle/icon.iconset"
fi

if [[ "$(uname)" != "Linux" ]]; then
  echo "This is for Linux; on a Mac, bundle/make-app.sh builds kessler.app." >&2
  exit 1
fi

if [[ -n "$PACKAGED" ]]; then
  if ! /sbin/ldconfig -p 2>/dev/null | grep 'libasound\.so\.2' >/dev/null; then
    echo "kessler needs ALSA's library: sudo apt install libasound2" >&2
    exit 1
  fi
elif [[ "${1:-}" != "--no-build" ]]; then
  if ! command -v cargo >/dev/null; then
    echo "Building needs Rust, which is not installed here. Easier: the ready-built" >&2
    echo "package from the releases (kessler-<version>-linux-$(uname -m | sed 's/aarch64/arm64/').tar.gz)," >&2
    echo "unpacked, then ./install.sh inside it. Or install Rust and run this again:" >&2
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
    exit 1
  fi
  if ! pkg-config --exists alsa 2>/dev/null; then
    echo "The build needs ALSA's headers first: sudo apt install pkg-config libasound2-dev" >&2
    exit 1
  fi
  echo "Building the release binary (on a Raspberry Pi the first time takes a while)..."
  ( cd "$ROOT" && cargo build --release )
fi
[[ -x "$BIN" ]] || { echo "No binary at ${BIN} -- run cargo build --release first" >&2; exit 1; }

case ":${XDG_CURRENT_DESKTOP:-}:" in
  *:KDE:*) SHAPED="${ICONS%/*}/icon-kde" ;;
  *:GNOME:*|*:GNOME-Classic:*|*:Unity:*|*:Budgie:*) SHAPED="${ICONS%/*}/icon-gnome" ;;
  *) SHAPED="" ;;
esac
[[ -n "$SHAPED" && -d "$SHAPED" ]] && ICONS="$SHAPED"

echo "Installing the icon..."
declare -A placed=()
for twice in "" "@2x"; do
  for file in "$ICONS"/icon_*x*"$twice".png; do
    [[ -e "$file" ]] || continue
    name="$(basename "$file" .png)"
    [[ -z "$twice" && "$name" == *@2x ]] && continue
    side="${name#icon_}"
    side="${side%%x*}"
    [[ -n "$twice" ]] && side=$((side * 2))
    [[ -n "${placed[$side]:-}" ]] && continue
    placed[$side]=1
    dir="$DATA/icons/hicolor/${side}x${side}/apps"
    mkdir -p "$dir"
    cp "$file" "$dir/kessler.png"
  done
done

echo "Writing the start menu entry..."
mkdir -p "$DATA/applications"
cat > "$DATA/applications/kessler.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=kessler
GenericName=Music Player
Comment=music player and ipod manager
Exec="$BIN" gui
Icon=kessler
Terminal=false
Categories=AudioVideo;Audio;Player;
StartupWMClass=kessler
EOF

mkdir -p "$HOME/.local/bin"
ln -sf "$BIN" "$HOME/.local/bin/kessler"

command -v update-desktop-database >/dev/null && update-desktop-database "$DATA/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$DATA/icons/hicolor" || true

echo "Done: kessler is in the start menu (under Sound & Video)."
echo "From a terminal: kessler gui   (~/.local/bin must be on PATH; it is after the next login)"
