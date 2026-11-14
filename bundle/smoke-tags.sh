#!/usr/bin/env bash

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
for tool in ffmpeg ffprobe python3; do
  command -v "$tool" >/dev/null || { echo "smoke-tags: $tool fehlt" >&2; exit 1; }
done

WORK="$(mktemp -d "${TMPDIR:-/tmp}/kessler-smoke.XXXXXX")"
LIB="$WORK/library"
mkdir -p "$LIB"
trap 'echo "(die testbibliothek liegt in $WORK)"' EXIT

cover="$WORK/cover.jpg"
ffmpeg -loglevel error -y -f lavfi -i "color=c=0x223344:s=300x300" -frames:v 1 "$cover"

put() { # put <dir> <file> <tone> <codec args…> -- <metadata args…>
  local dir="$LIB/$1" file="$2" hz="$3"; shift 3
  mkdir -p "$dir"
  ffmpeg -loglevel error -y -f lavfi -i "sine=frequency=$hz:duration=4" -ac 2 "$@" "$dir/$file"
}

mkdir -p "$LIB/Ajilvsga/The Harvest"
ffmpeg -loglevel error -y -f lavfi -i "sine=frequency=330:duration=4" -i "$cover" \
  -map 0 -map 1 -c:a libmp3lame -b:a 128k -c:v mjpeg -id3v2_version 3 -write_id3v1 1 \
  -metadata title="Trainwreck Afterbirth" -metadata artist="Ajilvsga" -metadata album="The Harvest" \
  -metadata date="2006" -metadata track="1/8" -metadata genre="Psych Folk / Drone" \
  -metadata:s:v title="Album cover" -metadata:s:v comment="Cover (front)" \
  "$LIB/Ajilvsga/The Harvest/01 Trainwreck Afterbirth.mp3"
put "Ajilvsga/The Harvest" "03 Making Space.mp3" 350 -c:a libmp3lame -b:a 128k -id3v2_version 4 \
  -metadata title="Making Space" -metadata artist="Ajilvsga" -metadata album="The Harvest" \
  -metadata date="2006" -metadata track="3/8" -metadata genre="Psych Folk / Drone"

python3 - "$LIB" <<'PY'
import os, subprocess, sys
lib = sys.argv[1]
def synchsafe(n): return bytes([(n >> 21) & 0x7f, (n >> 14) & 0x7f, (n >> 7) & 0x7f, n & 0x7f])
def frame(fid, data): return fid + synchsafe(len(data)) + b"\0\0" + data
def text(fid, words): return frame(fid, b"\x03" + words.encode())
def v1(title, artist, album, year, track):
    pad = lambda s, n: s.encode("latin-1")[:n].ljust(n, b"\0")
    return b"TAG" + pad(title, 30) + pad(artist, 30) + pad(album, 30) + pad(year, 4) + b"\0" * 28 + bytes([0, track, 255])
def mp3(path, hz):
    raw = subprocess.run(["ffmpeg", "-loglevel", "error", "-f", "lavfi", "-i", f"sine=frequency={hz}:duration=4",
                          "-ac", "2", "-c:a", "libmp3lame", "-b:a", "128k", "-id3v2_version", "0",
                          "-write_xing", "1", "-f", "mp3", "-"], capture_output=True, check=True).stdout
    return raw
def tag(frames):
    body = b"".join(frames)
    return b"ID3\x04\0\0" + synchsafe(len(body)) + body
d = os.path.join(lib, "Ajilvsga", "The Harvest")
front = tag([frame(b"TXXX", b"\x03PLAY_COUNTER\x003"), text(b"TCON", "Psych Folk / Drone"),
             text(b"TDRC", "2006"), text(b"TRCK", "02/08")])
open(os.path.join(d, "02 Cherry.mp3"), "wb").write(front + mp3(d, 340) + v1("Cherry", "Ajilvsga", "The Harvest", "2006", 2))
broken = b"TXXX" + synchsafe(4) + b"\0\x01" + b"\0\0\0\x04"
front = tag([text(b"TIT2", "Hatchet Man"), text(b"TPE1", "Ajilvsga"), text(b"TALB", "The Harvest"),
             text(b"TRCK", "4/8"), broken])
open(os.path.join(d, "04 Hatchet Man.mp3"), "wb").write(front + mp3(d, 360))
e = os.path.join(lib, "Ajilvsga", "Little Earth")
os.makedirs(e, exist_ok=True)
open(os.path.join(e, "01 Untitled.mp3"), "wb").write(mp3(e, 370) + v1("Untitled", "Ajilvsga", "Little Earth", "2009", 1))
PY

mkdir -p "$LIB/Alda/Tahoma"
for disc in 1 2; do
  ffmpeg -loglevel error -y -f lavfi -i "sine=frequency=$((400 + disc * 20)):duration=4" -i "$cover" \
    -map 0 -map 1 -c:a flac -c:v mjpeg -disposition:v attached_pic \
    -metadata title="Tahoma $disc" -metadata artist="Alda" -metadata album="Tahoma" -metadata album_artist="Alda" \
    -metadata date="2015" -metadata track="1" -metadata disc="$disc/2" -metadata genre="Folk" \
    "$LIB/Alda/Tahoma/$disc-01 Tahoma $disc.flac"
done
put "Kraftwerk/Ralf und Florian" "01 Tanzmusik.m4a" 440 -c:a aac -b:a 128k \
  -metadata title="Tanzmusik" -metadata artist="Kraftwerk" -metadata album="Ralf und Florian" \
  -metadata date="1973" -metadata track="1/6" -metadata genre="Elektronische Musik – früh"
put "坂本龍一/音楽図鑑" "01 Tibetan Dance.ogg" 460 -c:a libvorbis \
  -metadata title="Tibetan Dance" -metadata artist="坂本龍一" -metadata album="音楽図鑑" \
  -metadata date="1984" -metadata track="1" -metadata genre="テクノポップ"
put "Various Artists/Paysage" "01 Im Wald.ogg" 480 -c:a libvorbis \
  -metadata title="Im Wald" -metadata artist="Paysage d'Hiver" -metadata album="Paysage" \
  -metadata album_artist="Various Artists" -metadata date="2001" -metadata track="1"
put "Various Artists/Paysage" "02 Schnee.mp3" 500 -c:a libmp3lame -b:a 128k -id3v2_version 3 \
  -metadata title="Schnee" -metadata artist="Wintherr" -metadata album="Paysage" \
  -metadata album_artist="Various Artists" -metadata date="2001" -metadata track="2"

put "Éliane Radigue/Trilogie" "01 Kyema.wav" 520 -c:a pcm_s16le -write_id3v2 1 \
  -metadata title="Kyema" -metadata artist="Éliane Radigue" -metadata album="Trilogie" -metadata date="1988" -metadata track="1"
put "Éliane Radigue/Trilogie" "02 Kailasha.aiff" 540 -c:a pcm_s16be -write_id3v2 1 \
  -metadata title="Kailasha" -metadata artist="Éliane Radigue" -metadata album="Trilogie" -metadata date="1991" -metadata track="2"
put "Éliane Radigue/Trilogie" "03 Koume.aifc" 560 -c:a pcm_s16le -write_id3v2 1 \
  -metadata title="Koume" -metadata artist="Éliane Radigue" -metadata album="Trilogie" -metadata date="1993" -metadata track="3"
put "Various Artists/Paysage" "03 Draussen.opus" 560 -c:a libopus -metadata title="Draussen"

echo "testbibliothek: $(find "$LIB" -type f | wc -l | tr -d ' ') dateien in $LIB"

WEIRD="$WORK/weird"
mkdir -p "$WEIRD"
ffmpeg -loglevel error -y -f lavfi -i "nullsrc=s=3000x3000,geq=random(1)*255:128:128" -frames:v 1 -q:v 1 "$WORK/huge.jpg"
python3 - "$WEIRD" "$WORK/huge.jpg" <<'PY'
import os, subprocess, sys
out, huge = sys.argv[1], open(sys.argv[2], "rb").read()
def synchsafe(n): return bytes([(n >> 21) & 0x7f, (n >> 14) & 0x7f, (n >> 7) & 0x7f, n & 0x7f])
def be(n): return n.to_bytes(4, "big")
def f4(fid, data): return fid + synchsafe(len(data)) + b"\0\0" + data        # ID3v2.4 frame
def f3(fid, data): return fid + be(len(data)) + b"\0\0" + data               # ID3v2.3 frame
def t4(fid, words): return f4(fid, b"\x03" + words.encode())
def tag4(frames): body = b"".join(frames); return b"ID3\x04\0\0" + synchsafe(len(body)) + body
def tag3(frames): body = b"".join(frames); return b"ID3\x03\0\0" + synchsafe(len(body)) + body
def mp3(hz, seconds=4):
    return subprocess.run(["ffmpeg", "-loglevel", "error", "-f", "lavfi", "-i", f"sine=frequency={hz}:duration={seconds}",
                           "-ac", "2", "-c:a", "libmp3lame", "-b:a", "128k", "-id3v2_version", "0", "-f", "mp3", "-"],
                          capture_output=True, check=True).stdout
def names(title, artist="Weird", album="Weird Files"):
    return [t4(b"TIT2", title), t4(b"TPE1", artist), t4(b"TALB", album)]
def put(name, data, mode=None):
    path = os.path.join(out, name)
    open(path, "wb").write(data)
    if mode is not None: os.chmod(path, mode)

audio = mp3(600)
put("01 cut short.mp3", tag4(names("Cut Short")) + audio[: len(audio) // 2 + 7])
put("02 garbage in front.mp3", tag4(names("Garbage In Front")) + os.urandom(5000) + audio)
put("03 empty.mp3", b"")
put("04 not music.mp3", b"this is a shopping list, not an mp3\n" * 40)
claims = tag4(names("Claims Too Much"))
put("05 tag too big.mp3", b"ID3\x04\0\0" + synchsafe(len(audio) * 4) + claims[10:] + audio)
put("06 two tags.mp3", tag4(names("Two Tags")) + tag4([t4(b"TIT2", "Second Tag"), t4(b"TCON", "Noise")]) + audio)
v22 = b"".join(fid + len(b"\0" + w).to_bytes(3, "big") + b"\0" + w for fid, w in
               [(b"TT2", b"Old Tag"), (b"TP1", b"Weird"), (b"TAL", b"Weird Files")])
put("07 id3v2.2.mp3", b"ID3\x02\0\0" + synchsafe(len(v22)) + v22 + audio)
utf16 = lambda fid, w: f3(fid, b"\x01" + "\ufeff".encode("utf-16-le") + w.encode("utf-16-le") + b"\0\0")
put("08 utf-16.mp3", tag3([utf16(b"TIT2", "Grüße aus 東京"), utf16(b"TPE1", "Weird"), utf16(b"TALB", "Weird Files")]) + audio)
latin = lambda fid, w: f3(fid, b"\0" + w.encode("latin-1"))
put("09 latin-1.mp3", tag3([latin(b"TIT2", "Für Elise · à la française"), latin(b"TPE1", "Weird"), latin(b"TALB", "Weird Files")]) + audio)
put("10 strange letters.mp3", tag4(names("Tab\there, a bell\x07, emoji 🎧, עברית, " + "long " * 400)) + audio)
extra = [
    f4(b"USLT", b"\x03eng\0" + "la la la\nzweite zeile".encode()),
    f4(b"POPM", b"kim@example\0" + bytes([196]) + be(42)),
    f4(b"PRIV", b"com.apple.streaming\0" + bytes(range(16))),
    f4(b"COMM", b"\x03eng" + b"iTunNORM\0" + b" 000001A2 000001B3"),
    f4(b"COMM", b"\x03eng" + b"\0" + "ein kommentar".encode()),
    t4(b"TCOM", "Erik Satie"),
    t4(b"TIT1", "Gymnopédies"),
    t4(b"TBPM", "72"),
]
put("11 unknown frames.mp3", tag4(names("Unknown Frames") + extra) + audio)
ape_items = [(b"Title", b"In The APE Tag"), (b"Artist", b"Weird")]
items = b"".join(len(v).to_bytes(4, "little") + b"\0\0\0\0" + k + b"\0" + v for k, v in ape_items)
footer = lambda flags: b"APETAGEX" + (2000).to_bytes(4, "little") + (len(items) + 32).to_bytes(4, "little") + len(ape_items).to_bytes(4, "little") + flags + b"\0" * 8
put("12 ape tag.mp3", audio + items + footer(b"\0\0\0\0"))
put("13 huge cover.mp3", tag4(names("Huge Cover") + [f4(b"APIC", b"\0image/jpeg\0\x03\0" + huge)]) + audio)
put("14 read only.mp3", tag4(names("Read Only")) + audio, 0o444)
put("15 „quotes“ ü #%&.mp3", tag4(names("Odd File Name")) + audio)
PY
ffmpeg -loglevel error -y -f lavfi -i "sine=frequency=620:duration=4" -c:a flac -metadata title="Cut FLAC" -metadata artist="Weird" -metadata album="Weird Files" "$WORK/whole.flac"
head -c "$(( $(wc -c < "$WORK/whole.flac") / 2 ))" "$WORK/whole.flac" > "$WEIRD/16 cut short.flac"
ffmpeg -loglevel error -y -f lavfi -i "sine=frequency=640:duration=4" -c:a aac -movflags +faststart -metadata title="Fast Start" -metadata artist="Weird" -metadata album="Weird Files" "$WEIRD/17 fast start.m4a"
ffmpeg -loglevel error -y -f lavfi -i "sine=frequency=660:duration=4" -c:a pcm_s16le "$WEIRD/18 bare.wav"
echo "seltsame dateien: $(find "$WEIRD" -type f | wc -l | tr -d ' ') in $WEIRD"

cd "$ROOT"
KESSLER_SMOKE="$LIB" KESSLER_SMOKE_WEIRD="$WEIRD" cargo test --quiet --bin kessler tag_smoke -- --ignored --nocapture --test-threads=1
