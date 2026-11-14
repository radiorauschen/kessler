# kessler — the manual

everything kessler does, in detail: the window, the ipod, the config keys and
why things are the way they are. the short version is the [readme](../README.md).

## build

needs a rust toolchain (`rustup`, stable).

```sh
cargo build --release
./target/release/kessler --help
```

on a mac the window is drawn through metal (wgpu), elsewhere through opengl
(glow): on a mac opengl is a layer apple translates into metal itself, and
drawing into metal directly holds some 40–50 mb less. the menu's head says
which, small after the version number: `· metal` or `· opengl`. on linux the
other way is built in as well, to compare: `KESSLER_GRAFIK=wgpu kessler gui`
(or `=vulkan`) draws through wgpu, and the head says `wgpu · vulkan` — or
`wgpu · gl` where the machine has no vulkan. it makes the program some 5 mb
larger.

`cargo test` runs the tests. `./bundle/smoke-tags.sh` (needs ffmpeg,
ffprobe and python3) is the smoke test of tag editing on real files: it
builds a small library in every format and tag shape kessler reads — mp3
with id3v2.3, id3v2.4, id3v1 only, names only in the id3v1, a broken frame;
flac on two discs, m4a, ogg, wav, aiff, aiff-c, and an opus that must stay out — in a
temporary folder, scans it into a fresh database and edits it the way the
window does: one title, a whole album, an album renamed (its files follow
into a folder of the new name), an artist, disc and track numbers, japanese,
an emptied field, covers into every format. after every step it checks the
database's integrity, that every row is a file and every file a row, that
every file still decodes as long as it did, and that nothing it did not edit
has changed; then a second scan, which must find nothing new. a second part
does the same to eighteen weird and broken files — cut short, garbage in
front, empty, not music at all, a tag bigger than the file, two tags,
id3v2.2, utf-16 and latin-1, control characters and emoji, frames kessler
does not know, an ape tag, a huge cover, a read-only file, an odd file name:
each one an edit and a cover must either land cleanly or leave byte for byte
as it was, and iTunes' own frames (lyrics, rating, its loudness comment)
must survive.

platform notes:

- **macos** — nothing extra; audio goes through coreaudio.
- **linux** — needs alsa headers: `sudo apt install libasound2-dev` (or
  `alsa-lib-devel` on fedora). `./bundle/install-linux.sh` puts it in the
  start menu — see „on linux, and on a raspberry pi" below.
- **windows** — nothing extra; audio goes through wasapi.

## privacy

(written down here and in the readme; it was so from the start.)

kessler collects nothing. there is no telemetry, no analytics, no crash
reporting, no account and no advertising, and nothing about you or your
library is sent anywhere on its own. it goes online only for something you
asked for, and only to the place that has it:

- **covers, years and genres:** when you ask for them — cover art archive,
  the itunes store and deezer for covers, musicbrainz for years and genres.
- **artist information and lyrics:** while their column is open —
  musicbrainz, wikidata, wikipedia and last.fm for the artist, lrclib for
  the words.
- **radio and podcasts:** only once switched on — the stations and feeds you
  chose, radio browser when you search for a station, apple's podcast
  directory when you search for a podcast.
- **last.fm:** what you play is scrobbled only once you have signed in.
- **updates:** checked only when you click the row under `über`, one
  question to github.

each request carries only `kessler` and its version as its name; nothing
that tells you, your computer or your installation apart goes with it.

## usage

```sh
kessler                      # open the window (so does `kessler gui`)
kessler scan ~/Music         # import a folder into the library
kessler scan --prune         # rescan configured dirs, drop missing files
kessler stats                # track/artist/album counts
kessler paths                # where the config and database live
kessler -v scan ~/Music      # -v info, -vv debug (logs go to stderr)

kessler ipod list            # find mounted ipods
kessler ipod info            # capacity, track/playlist counts
kessler ipod tracks          # what is on the device
kessler ipod playlists
kessler ipod sync            # show what a sync would copy
kessler ipod sync --apply    # actually copy
kessler ipod sync --apply --transcode   # convert flac/ogg to mp3 on the way
kessler ipod backup          # copy the itunesdb aside
kessler ipod report          # the report the device column saves, printed

kessler toggle               # play or pause the running window (linux)
kessler play | pause | stop | next | prev
kessler now                  # what it plays, as one line, for a status bar
```

the `ipod` commands take a mount point (`kessler ipod info /Volumes/IPOD`) and
fall back to the single connected device when there is exactly one.

scanning is incremental: a file whose mtime matches the stored one is skipped,
so rescanning a large library is cheap.

the window's scan — at start, and on „jetzt neu einlesen" — also takes out
tracks whose files have gone from the music folders. a music folder that is
not there, or holds no music at all, is left alone: that is an unplugged disk,
not a library to empty. tracks brought in from outside the music folders are
never judged.

the tag editor (`e`) has the names, year, number, disc and genre, and
after them what itunes shows under „info" besides: composer, grouping, bpm
and comment. those four are not in the database; they are read from the
files when the editor opens (not for a selection of more than 400 tracks:
then they start empty and are written only where something is typed). the
comment is the one without a description — itunes' own comments, such as its
loudness („iTunNORM"), are left alone. an mp3 with only an id3v1 or an ape
tag gets an id3v2 beside it, which has room for all of this.

every write goes into a copy first. the copy replaces the file only if it
reads cleanly, plays as long as before, every edited field reads exactly as
typed, and every other field as it did; otherwise the file stays as it was
and the status line says which field would have changed. the id3v1 at the
end of an mp3 is written in latin-1, cut at thirty characters.

editing tags moves a file filed as `artist/album/file` inside a music
folder to the folders of its new artist and album, and removes the old ones
once they are empty. the file's own name changes with the title and number
only if it is the name kessler would have given it (`01 title.mp3`), so an
itunes `1-01 …` keeps its own. anything arranged by hand stays where it is.
the track keeps its lists, its plays and its place in the queue.

music.app keeps a library of its own, with its own copy of every tag and
every file's place. on a mac kessler tells it: a file kessler moved is
pointed at its new place, and every file kessler wrote tags, a year, a genre
or a cover into is read again (music's `refresh`), so music shows what the
file now says. the track is found in music by its title and then known by its
file, before anything moves. music is only asked while it is running — or
always, with `und an music.app` on, which starts it if need be.

## config

written on first run to `~/.config/kessler/config.toml`
(`~/Library/Application Support/kessler/` on macos — `kessler paths` prints
the real location).

a file kessler cannot read (a typo made by hand, half a file) is put aside as
`config.toml.kaputt` beside it, and kessler starts on the defaults and says so
in the status line. nothing in the old file is lost: fix it and move it back.

```toml
music_dirs = ["/Users/you/Music"]
watch_dirs = true      # read the folders again when the window opens

[ui]
ground = "pc engine"   # the colour scheme; menü → farben sets it

[player]
volume = 70
gapless = true
```

**a first start**: nothing switched on — not the music, the radio or the
podcasts — so the window opens on `alles ist ausgeschaltet` and says how to
switch them on; language as the system's, english where it says none; `公園`,
light or dark as the system is (light where it does not say); muto where the
build has it, else asap at width 85 and its own weight, the single-storey g,
the open zero; pictures screened, `blaues rauschen`, four tones, `feinheit`
at 0.6; `was läuft` `kompakt`, the library's figures bottom left, the status
bottom right, shuffle and repeat in the header, the scope in the search
field; `alle-zeile`, `zuletzt-zeile` (80 tracks, opening on `zuletzt
hinzugefügt`) and `kompilationen-zeile` shown, no counts beside the artists;
the albums as headings in the track list with covers of 60 points and their
counts, whole compilations; `nr` (flush left), `jahr`, `genre` and `länge`
in the track list; as many frames as the screen draws (30 on a raspberry
pi), the scope at 20; no screen saver. a config file that is there keeps
what it says.

nearly everything in the file is set from the menu and written back by the
window; it is only worth opening for the few things that have no row there
(`font_dir`, `font_fallback`). keys an older version wrote and that mean
nothing any more — `theme`, `glyphs`, `[keys]`, `replay_gain` — are read past
and dropped the next time the file is saved.

## keys

| key | action |
| --- | --- |
| `space` | play / pause |
| `enter` | play selection — several tracks picked with `shift` + `j` `k`: all of them, from the first |
| `n` / `p` | next / previous track — the one before in the queue. shuffled, back to what played before. more than three seconds in, `p` starts the track again|
| `1` `2` `3` `4` | the library, the radio, the podcasts, the ipod — each on its own key, as far as it is there (the podcasts on `3` and the ipod on `4`)|
| `v` / `shift` + `F10` / the menu key | the right-click menu for the row the keyboard stands on — a track (or the tracks picked), a record, a name, a row of the queue or a list, a list, a station, an episode — opened beside that row. in it `↑` `↓` (`k` `j`, `tab`) go along the entries, past those greyed out, and on to the stars at the foot of a track's menu, where `←` `→` (`h` `l`) give one star less or more at once; `enter` does the one stood on (on the stars, closes the menu), `esc` (or the same key) closes it, as it now closes a menu a right-click opened|
| `c` then `1` `2` `3` | switch the music, the radio or the podcasts off, as under menü → allgemein → funktionen (a view switched off while open goes back to the one open before it, where that is still on — the radio after the podcasts opened from it): within a second of `c`, or while `c` is held, for as many digits as are pressed under it. the foot says how to switch it back on — or, for a view off already, that it is (`podcasts sind schon aus — 3 schaltet sie ein`)|
| `1` `2` `3` on a view switched off | switch it on again and open it (the same digit twice within a second)|
| `j` `k` / `↓` `↑` | move down / up (`shift` to select several) |
| `cmd` + `a` | select all: every track, from the tracks or the albums; every row, from the queue. not in the artists, nor in an open list (`ctrl` + `a` on linux) |
| `esc` | select nothing: no track, no row of the queue or a list is marked, the cursor's neither. the next step, click or column marks the cursor's row again. a click beside the rows of the tracks, the queue or a list does the same. while the queue or a list has the keys, the track list keeps its cursor but does not mark it|
| `h` `l` / `←` `→`, `tab` | previous / next column |
| `g` `g` / `shift` + `g` | to the top / the bottom of the column, as in vim |
| `shift` + `←` `→` | seek ten seconds |
| `,` / `.` | seek a minute back / on |
| `/` | search (`enter` or `tab` back to the list, `esc` to leave); in the field `←` `→` move the caret, `alt` with them a word, `cmd` with them or home / end to either end|
| on the radio | `j` `k` / `↓` `↑` the station, or the line in „gerade gehört"; `h` `l` / `←` `→` between the stations and „gerade gehört"; `enter` plays the station, or keeps the line heard (or lets it go), or shows who sang a song kept; `d` / `backspace` forgets a song kept|
| on the podcasts | `j` `k` / `↓` `↑` the next row of the first column — „alle", „offen", „geladen", „gemerkt", a podcast — chosen at once, or the next episode looked at; `h` `l` / `←` `→` / `tab` between the podcasts, the episodes and the queue; `enter` into the episodes, or plays the episode; `shift` + `↓` `↑` picks a run of episodes, `cmd` + `a` all of the list — picked, the right-click menu does to them all; `a` into the queue or out; `d` / `backspace` archives or brings back; `esc` lets the picked go; `gg` / `G` top and foot. in the queue, as in the music's: its played rows, the episode playing and what comes are one list for `j` `k`, arriving there on the one playing — with a cursor of its own, so the episodes and the episode's page stay as they are while it moves; `d` / `backspace` take the row out instead of archiving it, `shift` + `j` `k` carry a row of what comes up or down, `shift` + `d` twice empties it all, `enter` plays (the one playing plays on); `shift` + `↓` `↑` picks a run of rows across played and to come, `cmd` + `a` all of them — `d` takes them all out, `shift` + `j` `k` carries those to come together, `esc` lets them go |
| `a` / `shift` + `a` | queue: at the end / next |
| `d` / `backspace` | in the queue: take the track out |
| `cmd` + `backspace` | the same, as the finder takes things away (`ctrl` + `backspace` on linux) |
| `shift` + `d`, twice | in the queue: empty it, all but what is playing. the first press asks („shift + d again to clear“), the second within two seconds clears, as `q` quits |
| `shift` + `j` `k` | in the queue: move the track (or the picked run) down / up |
| `shift` + `↓` `↑` | in the queue: pick a run of tracks |
| `w` | show the queue — not over the radio nor the ipod, whose columns stand where it would|
| `u` | lists instead of the queue |
| `shift` + `u` | open the next list |
| `i` | about the artist |
| `o` | other artists: like yours, not in the library yet |
| `b` | the screen saver, now |
| `y` | the lyrics of the song |
| `z` | the equalizer |
| `e` | edit tags |
| `t` | the log |
| `s` / `r` | shuffle / repeat |
| `+` / `−` | volume |
| `shift` + `t` | light ↔ dark |
| `shift` + `f` | next colour scheme, light or dark as it is |
| `alt` + `shift` + `f` | previous colour scheme |
| `f` | full screen (also `cmd` + `ctrl` + `f` on a mac, `F11` on linux) |
| `cmd` + `+` `−` `0` | everything bigger / smaller / as the system has it |
| `m` | open the menu: every setting |
| `?` | every key at a glance |
| `q` `q` | quit (twice within two seconds) |

menu → allgemein → `tasten` → `alle zeigen` and `?` both lay the list over
the window (from the menu, the menu fades out first and the list fades in,
the veil staying), all of it at once, with no heading and nothing set in;
`?`, `esc` or a click puts it away. only on a screen too short for all of it
does it scroll — the wheel, or the arrows. the sheet has a search: whatever
is typed while it is up goes into the field on its first line, no click
needed. a key answers when every typed word is one of its keys (from two
letters on, the beginning of one, so `shi` finds `shift`) or begins a word
of what it does or of its group, as shown in the window's language. `esc`
empties the field, a second `esc` puts the sheet away; `?` closes it only
while nothing is typed. the keys that do not answer are set back where they
stand, so the sheet keeps its size and shape. `j` `k` no longer scroll the
sheet — they are letters for the field.

in the search, `enter` and `tab` leave the line with the words standing and the
list still filtered, and hand the keys back: `j` `k`, `a` and `enter` then act
on what was found. `enter` used to play the selection straight from the line.

emptying the queue (its head under the pointer) leaves the track
that is playing; it plays on to its end. with nothing playing, all of it goes.
the artist's column follows the queue's cursor while the keyboard is in the
queue.

several rows of the queue can be picked at once: `shift` + click or `shift` +
`↓` `↑` takes a run from the cursor, `cmd` (or `ctrl`) + click one more row.
`d` or `backspace` then takes all of them out, and `shift` + `j` `k` carries a
run as one block. a plain step or click lets them go, and so does the queue
changing from elsewhere.

the menu is set in tabs: the tabs down the left — `bibliothek`,
`darstellung`, `spalten`, `wiedergabe`, `ipod`, `über`, the keyboard's only
while one is plugged in (the ipod's always) — and what the chosen one holds
beside them, the sheet as tall as the tallest tab, so it does not jump from
one to the next. every tab is measured as it opens, so it stands in the
middle of the window from the start; before, it knew only the tabs it had
shown, and grew downwards off centre when a taller one was chosen. a window
too narrow for the tabs down the side has them along the top instead, on a
second line where they do not fit on one (before, the sections stood one
under the other) — the open one underlined as in the masthead's switch, all
of them in the regular cut. only what the tab holds scrolls: the tabs stay
where they are, and each keeps its own place in it (before, the next tab
opened as far down as the last had been scrolled). the sheet keeps the
height it opened at: a list folding out under a row — the colour schemes,
the keys — is scrolled through inside it instead of the sheet growing. a
window resized has it measured afresh.

a section's heading that would only repeat the name of its tab is left out,
so the first row stands level with the tab beside it; the headings of
further sections in a tab (`idtags ergänzen`, `kopfzeile`, `last.fm`, the
three of `spalten`) stay. those three say only which column —
`künstler:innen`, `alben`, `titelliste` — the tab says the rest; they were
`spalte künstler:innen`, `spalte alben` and `spalten der titelliste`.

the sheet has a head: the program's figure at the left — the vectorscope's
figure, still, or moving with the music while something plays — with
`kessler` beside it and under that, at the label's size, its number and the
build's commit (before, the number as large as the name and the commit after
it; a development build says `· dev` after the number, and its window is
called `kessler dev`) — the three a shade more widely spaced than their
sizes elsewhere, 0.4 points a letter — and `schliessen` at the far end, set
like `menü`: a click closes the menu, for whoever does not know the keys.

it is worked from the keyboard like the columns: the tabs are one column and
what the open one holds the other. `h` `l` (or `←` `→`) go from one to the
other — the open tab wears the marker while the keyboard is in the tabs,
which is where each opening starts (before, the first `↓` went to the first
row) — and `j` `k` (or `↓` `↑`, or `tab`) move within: from tab to tab, or a
cursor through every row of the tab that does something, and the sheet
scrolls to keep it in sight. on a slider `h` and `l` move it a step.
`enter`, `space` or `l` do what a click on the row does, and from a tab go
into it. from anywhere, `shift` + `h` `l` (or `←` `→`) and `[` `]` go to the
tab before and after, and a digit to that tab. the page keys go ten rows at
a time, `home` and `end` to the first and the last. `esc` or `m` closes it.

in a field — the tag editor, a list's name, the artist's form — `cmd` + `a`
marks the whole value: the next letter replaces it, `backspace` takes it
away, `cmd` + `c` copies it, `cmd` + `x` cuts it, and an arrow puts the
caret at that end instead. `cmd` + `backspace` empties the field outright.

## supported formats

mp3, aac and apple lossless (both `.m4a`), flac, ogg vorbis, wav and aiff
(`.aif`, `.aiff`, and aiff-c as `.aifc`) — what rodio decodes with the
symphonia codecs built in (rodio 0.23 with symphonia's simd paths; no
recording, no dithering of the output). before that an `.m4a` from itunes'
lossless import said „unrecognized format". aiff-c (garageband, logic and
macos write it, often little-endian `sowt`) plays uncompressed, as 32- or
64-bit float, a-law and µ-law; only ima4-compressed aiff-c, from old mac
sound files, does not. the format column says `aiff` for both, and sync
converts an `.aifc` for the ipod like a flac (the firmware is only sure to
take plain aiff). not yet: opus, wavpack, monkey's audio, musepack, dsd and
wma — a scan counts them per format, so the next one worth the work is
known.

a file that cannot be played is never put in the queue by a double-click,
and choosing one — a row of the queue, the next track — while something
plays leaves the queue and the header on what is heard; the status line
says why.
`SUPPORTED_EXTENSIONS` in `src/library/metadata.rs` is deliberately narrow:
listing a format there that playback cannot handle only imports files that
fail later.

## ipod

**reading** works on any ipod with a little-endian database: every track with
its metadata, rating, play count and file path, plus all playlists in their
stored order.

**writing** depends on the model. from the ipod classic (2007) and nano 3g
onward the firmware expects the database to carry an hmac-sha1 signature keyed
to the device's firewire guid. kessler cannot compute that yet, and a database
without it makes those models show an *empty library* even though every file is
still on the disk — so `kessler ipod sync` identifies the hardware first and
refuses rather than breaking it. `kessler ipod info` prints the verdict.

the master playlist carries the library's indices as itunes
writes them (`mhod` 52 for title, artist, album, genre and composer, without
jump tables, as apple writes them), and lists the tracks in the artist index's
order. a 3g builds its artists, albums and genres menus from them; without
them it walks the tracks in the order they were added and folds only
neighbours together, so an artist synced in several goes stands in the menu
several times, with every record under each. names go onto the device
trimmed, each run of spaces one space, invisible characters left out. a
device whose database is not yet in that order is written again by the next
sync even with nothing to copy (`menüs: neu ordnen`).

the artist column (library, ipod, lists) and the track list
sorted by artist run in the same order as the ipod's own menus, the one
apple's software writes into the indices: what stands before the first letter
or digit does not count („(((Microwaves)))" under m), accents fold („ÄÄNIPÄÄ"
among the a), names beginning with a digit after the letters (as apple has
them), numbers by their size (6955 before 40318), and
a nameless artist last (`db::shelf_key`).

the tracks are numbered afresh whenever their numbers would not
rise down the track list. a 3g finds a track by its number as if they rose, as
they do in every database itunes writes; with the list in the artists' order a
track copied over later would carry a higher number than those after it, and
the 3g's artists menu would stop at the first such track. a database whose numbers do
not rise is written again by the next sync, and the report says `titelnummern
steigen: ja/nein`. every text in the database carries the word
apple and libgpod set to 1 beside its length, and a copied track names its
kind (`mhod` 6, „MPEG audio file" and the like), as itunes does.

a device with an `iPod_Control` folder but no database yet — one
just restored in the finder, say — counts as an ipod: it reads as an empty
library, and the first sync makes the folders and the database, as libgpod's
`itdb_init_ipod` does. to fill an ipod anew from nothing: restore it in the
finder, then sync it with kessler. the report says `noch keine` under
`datenbank`.

every track goes on ticked (0 at its entry's `checked` byte, as itunes writes
it; an older ipod passes over an unticked track when it plays a record on) and
marked as audio. a database with unticked tracks is written again by the
next sync. each copy first asks the device whether it still has room, so
converted files, whose size is not known before, cannot fill it. a sync
leaves a backup of the database beside it (`iTunesDB.kessler-backup.<time>`);
only the newest 10 stay. `beim anschliessen: abgleichen` does nothing when
there is nothing to do.

| model | sync |
| --- | --- |
| ipod 1g–4g, photo, 5g/5.5g video, mini, nano 1–2 | works |
| ipod shuffle 1g–4g | works, with its `iTunesSD`, no voiceover |
| ipod classic (6g/7g), nano 3g–4g | refused — needs a signed database |
| nano 5g+, touch, iphone | refused — different format entirely |
| unrecognised model | refused, on purpose |

a shuffle has no screen and never reads the `iTunesDB`: it plays what
`iPod_Control/iTunes/iTunesSD` lists. every write of the database on a shuffle writes the `iTunesSD` from it
too — the plain one of the first and second generation, the chunked one with
playlists of the third and fourth — and clears the shuffle's `iTunesPState`,
`iTunesShuffle` and `iTunesStats`, which point into the old one by position.
the order it plays in without shuffling is album artist, album, disc,
number. voiceover is left out: the third and fourth generation play as with
voiceover turned off. not yet tried on a real shuffle.

a shuffle is known by the model number in `iPod_Control/Device/SysInfo`,
the fourth's later colours (2012 to 2015) included. when that
number is missing or unknown, kessler looks for the `iTunesSD` only a
shuffle has: one beginning `bdhs` is taken for a fourth, one of a short head
and 558 bytes a track for a second (each pair writes the same list, so
only the picture can be wrong). a device kessler does not recognise says
„modell … unbekannt" in its column.

a shuffle is treated plainly. in the device column it turns as itself
rather than as the player with a screen — the white stick of the first,
the square clip of the second, the bare stick of the third, the small
square of the fourth, no cover on any. a sync copies and removes music and
leaves tags and sound check on the device as they are, since nothing on a
shuffle shows them; the first and second have no playlists, so they get no
„zuletzt hinzugefügt" and their column says nothing of lists.

the last row matters: an unknown ipod is treated as one that *might* need a
signature. refusing is recoverable; writing an unsigned database to a device
that wanted one is not.

### the ipod in the window

plug one in and its name appears in the header, next to the menu — nothing
else moves; the window stays on the library. click the name and the window
reads the device instead: the same three columns, the same search, the same
rows — read from the device's own database rather than from sql — with the
device's panel in the fourth column, in the window's own colours. `1` `2`
`3` `4` choose the words of the switch at the top — `musik`, `radio` when it
is on, `podcasts` when they are on, `ipod` while one is plugged in — each
always on its own key (the keys counted along the words there were, and the
podcasts stood last). click the name again, or `schliessen` in the panel,
and it is the library again. the device's panel is only ever there with the
device's shelf: library and device are kept strictly apart, so the device's
line in the foot goes over to the device too, and so does anything sent to
it from the library.

everything you would otherwise do in a terminal is in the window too:

- **menü → ipod** — which device it is, which generation and model number, how
  full it is, how many tracks and playlists it carries. on a model that cannot
  be written to, one row says so instead of offering to. with nothing plugged
  in, the tab is there all the same: it says `nicht angeschlossen`, and what
  is set there holds for the next ipod the moment it is plugged in.
- **`beim anschliessen`** (`ipod_on_connect`) — what happens when an ipod is
  plugged in, or is already when the window opens: `nichts` (the default),
  `fragen` — the overview `überspielen: ganze bibliothek…` opens, to be
  answered — or `abgleichen`, that sync straight away, without asking,
  converting as `flac umwandeln` says. never on a model that cannot be written
  to, and never over a transfer already under way.
- **`gespielt & bewertet`** (`ipod_take_plays`, on; only counts and ratings,
  never files; says `in bibliothek und last.fm`, or `in die bibliothek`
  without last.fm, or `aus`) — what the ipod played and rated while it was
  away comes in when it is plugged in, before anything else happens: the plays
  are added to the library's counts (and its history, spaced a track's length
  apart, the last at the time the ipod noted), a rating set on the ipod
  becomes the library's, and with last.fm connected each play is scrobbled at
  the time it was heard — those older than two weeks only into the library,
  since last.fm files nothing older. the ipod counts in a file of its own
  beside its database, `iPod_Control/iTunes/Play Counts`; kessler only reads
  it on plugging in, and remembers in `ipod-plays.toml` beside the library
  what it has taken, so a second plug-in before the next sync takes only what
  is new. the log says how many plays came in, how many went to last.fm and
  how many belonged to tracks the library does not have. whenever kessler
  writes the ipod's database — a sync, a removal — it folds the file's counts
  into it first and takes the file away after.
- **`überspielen: ganze bibliothek…`** — the same as `kessler ipod sync
  --apply`, by way of a panel that says what it *would* copy: how many tracks,
  how many megabytes, what is already there, how much room would be left.
  nothing is written until that panel is answered.
- **`datenbank: sichern`** — `kessler ipod backup`. a sync makes a backup by
  itself before it writes; this is for making one when you are not about to.
- **`auswerfen`** — the same word the device's own footer carries.

a selection goes over without the menu: right-click tracks or a record →
`auf den ipod`, which goes over to the device and opens the same panel there
for that selection. or drag them — tracks, a record by its cover or name —
to `ipod` in the header, as the music app takes them on the device in its
sidebar: while tracks are carried the word opens into a place to put them
down that says `hierher ziehen zum hinzufügen` (as wide as the words need),
pushing what stands left of it aside, and darkens with them over it; letting
go goes over to the device with the panel. not while a copy or a removal is
under way, and not to a device kessler does not write. a plan still waiting
for its yes in the ipod's column, or the result of the last sync, does not
stand in the way (a removal waiting for its yes neither): the dropped tracks
replace it with a plan of their own. when `ipod` stays shut, the status line
says why as soon as the tracks are picked up.

while an ipod is plugged in, the library's track list marks what is on it
already: the word „ipod" beside the title, a label like „flac", where that is
one too after it. what counts as on it is what a sync would leave be, found
the same way. menu → ipod → `markierung in der bibliothek` (`[ui]
device_mark`) chooses the mark: `wort „ipod“`
(the default), `punkt` (a small dot instead) or `aus`. the device's own shelf
carries no mark — everything on it is on it. so a library bigger than the ipod
goes over a piece at a time: right-click what is not marked → `auf den ipod…`.

**taking tracks off.** on the ipod's own shelf, right-click tracks, a
record or an artist → `vom ipod entfernen…`. the device's column asks first — what goes,
how much room it gives back — and only `entfernen` does it: the database is
backed up, written without those tracks (and without them in every
playlist), and then their files are deleted. the library is not touched.

the panel — the plan, then the copy under way, then what came of it — stands
in the device's own column, under what the device says about itself, so the
records and their tracks stay where they are. while the copy runs, the space
line and its bar follow it, read twice a second.

### formats

no ipod of any generation plays flac or ogg vorbis. copying one across is a
*silent* failure — the file transfers, takes up space, and the device simply
never lists it. so sync checks each track against what the identified hardware
can actually decode and sets the rest aside:

| generation | plays |
| --- | --- |
| ipod 1g–2g | mp3, wav, aiff |
| ipod 3g | the above plus aac/m4a |
| ipod shuffle | mp3, aac/m4a, wav — aiff is converted |
| 4g and later | the above plus apple lossless |

**on a mac nothing has to be installed to convert.** macos brings its own
converter, `afconvert` (core audio): it reads flac, wav, aiff, aiff-c and apple
lossless and writes aac, which every ipod from the third generation on
plays. kessler converts to **aac at 256 kbit/s, 44.1 khz** with it and then
writes the library's tags into the new file. only what core audio cannot
read — ogg vorbis — and devices older than the third generation, which have
no aac, need **ffmpeg** (`brew install ffmpeg`, found on `PATH` or where
homebrew and macports put it); it converts to 320 kbit/s mp3. on linux and
windows ffmpeg does all of it. originals are never touched — the conversion
happens in a temporary directory that is cleaned up afterwards.

in the window it is **menu → ipod → `flac umwandeln`** (`ipod_convert`, on
by default): with it on, `überspielen` converts on the way and the plan says
so before anything is copied — `umwandeln: 123 titel — flac → aac`. off, the
plan offers `überspielen` and `mit umwandeln` side by side. the device's
column says `wird umgewandelt` instead of `spielt es nicht` while it is on.
`kessler ipod sync --transcode` does the same from a terminal.

**podcasts on an old ipod** — an experiment: **menu → ipod →
`podcasts als playlist (experiment)`** (`ipod_podcasts`, off by default; the
row is there while the podcasts are switched on). the old ipods have no
podcasts of their own, but they have playlists. with it on, `überspielen:
ganze bibliothek` also takes the episodes that are fetched and not heard yet
— the ones under `geladen`, without the heard and the archived, the kept ones
always — and writes a playlist `podcasts` of them, the newest first. on the
device an episode is a track like any other: its title, its podcast as artist
and record, the genre `podcast`. what the device cannot play is converted as
the music is. an episode heard or archived since goes from the device again
at the next such sync, its file with it, and the plan says so (`… gehörte
folgen kommen weg`) — only the episodes kessler put there itself; this is
the one thing a sync deletes. not on a shuffle, which has no playlists, and
where an episode was left on the device does not come back to kessler.

a converted file is deleted as soon as it is on the device. the new database
is written out to the disk in full before it takes the old one's place, and a
track whose folder cannot be made counts as failed by itself — the sync goes
on and writes the database for what did arrive.

note what this does to matching. a flac in the library and its mp3 on the ipod
have nothing in common size-wise, so sync compares artist, album, title and
**duration**, rounded to whole seconds. matching on file size would re-copy
every converted track on every sync, forever.

### what sync does

it is **additive** for music. tracks already on the ipod stay there whether
or not the library still has them, and a sync never deletes a file on the
device — that would mean removing files a user may have no other copy of.
taking tracks off is a thing of its own, asked for by hand (above). `kessler ipod
sync` without `--apply` only prints the plan.

**playlists are the library's.** in the window, `synchronisieren…` (the whole
library) takes every playlist off the device but its library list and
`zuletzt hinzugefügt`; the plan names each one before anything is written,
and the column says `alte playlists: n · kommen weg`. the music in them stays.
podcast lists are left alone.

**tags follow the library.** a track already on the device whose tags have
changed in kessler — title, artist, album, album artist, genre, year, track
and disc number — gets the new ones in the device's database; the file is not
copied again. a field the library leaves empty takes nothing off the device.
the column says `idtags: n geändert`, the plan `n titel neu getaggt`.

**sound check comes along.** where kessler knows how loud a track is (see
*levelling*), it writes the ipod's sound check figure for it — with a copy,
and for tracks already there as an update of the database, like tags. turn
sound check on in the ipod's own settings and it levels the same way.
the column says `sound check: n neu`.

matching goes three ways, in order. a track kessler copied carries the
library's id in its persistent id (`KESS` and the id), and is found by it
whatever its tags say now; copies by older versions are re-stamped when they
are found. then artist, album, title and length in whole seconds — not paths,
which differ on each side, and not file size, which a conversion changes.
last, for a track whose tags changed before it carried the id: the same
length, and artist and album with a nearly-same title, or the same title with
artist or album — taken only when exactly one track on the device fits and
nothing else has claimed it. the worst case is a duplicate, not a lost track.

the order of operations is chosen so an interrupted sync cannot break the
device: the old database is copied aside first, then files are copied, then the
new database is written to a temporary file and renamed into place. kessler
also parses its own output before installing it — a database it cannot read
back is one the ipod would not like either. pull the cable halfway through and
the ipod still has its old, working database and a few unreferenced files.

two database shapes are recognised but not supported, and say so rather than
producing garbage: zlib-compressed databases (newer ipods and ios devices) and
byte-reversed big-endian ones.

the format is implemented from libgpod's `itdb_itunesdb.c`. it is worth noting
that several widely-copied web tables of this format are wrong in two places —
timestamps count from **1904**, not 1970, and string payloads carry an encoding
marker with utf-16le text rather than being plain utf-8. `src/ipod/itunesdb/`
follows the reference implementation instead.

tests build a synthetic itunesdb byte by byte (`test_writer.rs`) and parse it
back, which pins down every offset without needing a device attached. the
writer is then checked against *that* fixture rather than only against its own
output — reading a database this code did not write, writing it out and reading
it again would not survive a misunderstanding shared by reader and writer.

## layout

```
src/
  main.rs        cli entry point (clap): the window, and scan/stats/paths/
                 lastfm/ipod subcommands
  config.rs      toml config, xdg-ish paths
  error.rs       KesslerError
  db/            sqlite schema, queries, models
  library/       scanner (walkdir), metadata and tags (lofty), covers, years
  player/        rodio audio engine, play queue, shuffle/repeat
  lastfm/        scrobbling
  media/         the system's media keys and now-playing (macos; linux by mpris)
  ipod/          device detection, model table, itunesdb reader/writer, sync
  gui/           window frontend (egui)
    mod.rs       the window's state (GuiApp) and its frame loop
    app/         what the window does, one `impl GuiApp` per subject
    library.rs   drawing: the columns, the masthead, the fourth column,
    library/       the forms and the right-click menu
    menu.rs      the menu sheet
    theme.rs     design tokens; text.rs, every piece of text
```

`gui/` sits behind the `gui` cargo feature, on by default; without it the
program is its subcommands. `db`, `library`, `player` and `ipod` know nothing
about the window.

the ipod code sits behind the `ipod` cargo feature, on by default;
`cargo build --no-default-features` leaves it out entirely.

## the window

`kessler gui` opens the same library in a window, with the mouse working:
click a row to select it, double-click a track to play it (at the end of the
queue — see below), drag along the
progress rule to scrub through a track.

it is built to one deliberately narrow set of rules, which live in
`src/gui/theme.rs`:

- **two type sizes.** 15px for content, 11px for labels. there is no third,
  and a test fails if one appears.
- **two weights**, regular and bold.
- **one sign for the pointer**: whatever does something gets the light plate
  under it, eight points round the ink of its words. the arrows to the
  neighbouring columns keep their own tone. the status line, the device line
  and `auswerfen` turn to the ink, as `menü` does (news keeps the accent).
- **the status line** sets who speaks a little apart from what they say:
  `last.fm 0:30`, `ipod 14 titel`.
- **three tonal values** plus one accent, and the accent says one of two
  things only: this is what plays, or this is news — a status, an error, a
  scan under way.
- **lowercase throughout**, after the hfg ulm — which set its own name that
  way. the practical gain in a list of thousands of rows is that without
  capitals the line is quieter and word shapes stay easier to tell apart.
  the one exception is the tag editor, which shows what will be written into
  the file, capitals and all.
- **no lines.** columns stand apart by whitespace. a plate says where the
  hand is: a light one under the pointer, a darker one — the marker, a
  highlighter stroke the width of the column — under the keyboard's row and
  the one just clicked, its name regular. weight says what is chosen where
  the hand is not: the artist whose albums are showing, the album whose
  tracks are, in bold and without a plate (before, those wore a fainter
  marker and stayed regular, and the keyboard's row was bold). a chosen or
  playing name stays bold under either plate too — the pointer's the
  keyboard's; before, it went regular, and in a face whose weights are not
  one width — muto — it grew and shrank as the hand passed. the menu's tabs
  follow the same three states. the one rule in the whole interface is the
  two-pixel progress bar, which turns out always to be carrying something:
  how far into a track you are, or that there is no track.

because size is off the table as a device, hierarchy comes from four axes and
each does exactly one job: weight says what is selected, tone says what
recedes, letter spacing says what is a label rather than content, and the
plates say where the hand is. weight applies to names only — the
artist, the album, the title. a track number set in bold does not make its row
easier to find; it only makes the column of figures look uneven.

what is playing wears the accent in all three columns at once — its artist,
its record and its title — so the three read as one line across the window,
and, with `pfeile` on, the playing track has a `→` before its title, after the number. a hovered name is a
grey plate and nothing more, so the two can no longer be mistaken for each
other.

the window gives ground as it shrinks, and it gives it in a fixed order: the
leading between rows tightens as the window loses height, and the two name
columns give up width before the track list does, because the track list is the
one column whose content has no natural length. the two type sizes never move.

lists fade out at the top and bottom rather than ending at a cut edge — but
only once there is something scrolled past, so an unscrolled list never has
a washed-out first row. the fade reaches 28 points in, as does the air under
a list's last row. the fade is all there after a few points scrolled. moving
the selection scrolls the column with it, keeping three rows of daylight
above and below, so it never walks off the edge. while the pointer is over a
column, a thin mark just outside its right edge — five and a half points
out, half way between a marked row's plate and the divider — says how long
the list is and where in it you are. it can be taken hold of the way a mac's
own scroller can: pull it to scroll, or press beside it to jump there. the
line that appears in the gap between two columns — pull it to change their
widths, click it to share the row out evenly — is the same thickness and the
same rounded shape.

`/` opens a filter line under the masthead — click the `/ suchen` line to do
the same. it narrows all three columns at once and says how much is left.
words are anded, so `eno green` finds *another green world* and nothing
else, and accents are optional on both sides: `bjork` finds björk and so
does `björk`. `Esc` clears it. `←` `→` move the caret through what is typed,
`alt` with them a word at a time, `cmd` with them (or home and end) to
either end; a letter goes in, `backspace` and `delete` come out, at the
caret, and the letter under the block stays legible in it. `↑` `↓` still
move through the list.

the column heads are the sort control. click one to sort by it, click it again
to turn it round. every head stands in the same tone, the one holding the
order too — its arrow says so — and under the pointer wears the light plate
the words in the header do. `vol` in the top line has a rule to drag beside
it.

both rules — the timeline and `vol` — **swell** under the pointer instead of
switching states: over about an eighth of a second the line thickens from
two points to four and the knob grows out of it. at rest the knob is still
there and is exactly as thick as the line, which is the same thing as being
the line, so nothing appears out of nowhere and nothing pops. it sinks back
the same way when the pointer leaves. their ends are rounded, the knob a
disc — in muto (and muto mono) square, as is the mark beside a scrolled
list: the face's own ends are square; the knob there an upright bar as four
points wide and twelve tall. the bar keeps its width all the way and only
grows in height, from the line's thickness, so it slides out of the rule
above and below rather than growing out of a point.

a quick flick on a trackpad coasts a quarter of a second rather than half,
so a list thrown against its end springs back less far — the bounce came
with egui 0.37. pushed past its end with a trackpad or a touch mouse, only a
seventh of the push gets through, so a list gives a little and springs back,
eased, as the system's own lists do.

a title too long for its column scrolls sideways while it is selected or under
the pointer, waits a moment first so that sweeping down a list does not set
every row in motion, and slides back when you leave. either way it fades into
the row's own background rather than ending on a cut-off letter. so do a
record's name and the line under it beside its cover in the combined view,
while the pointer is on the heading. the line
saying what is playing behaves the same way; there, title, artist and album are
laid out as a single line that happens to change colour, so a long one scrolls
as a whole instead of as three pieces sliding apart.

on a mac the window's three buttons stand in from the corner, where a mac
puts them in a window with a toolbar, in a band 56 points tall. kessler sets
them there itself, every frame, with no toolbar: a toolbar would take every
click in its band, and the masthead's first line stands in it — the search
field, flatter (22 points rather than 28) and starting clear of the buttons,
on their middle, and the rest of the line — in every view but `kompakt`,
which keeps its line under the band and sends only `menü` up into its
right-hand corner. a narrow window has its line under the band too. the
band's empty parts move the window, and only they: the mac itself would move
it from anywhere in the band, the timeline and the volume included, so
kessler's view tells it not to. a double click there makes it as big as the
screen and back, at once: the window is the size of the screen in the next
frame, and nothing is stretched. the mac's own zoom holds kessler still
while it runs, so the window's last picture is stretched to the growing
frame and everything jumps back to its size at the end.

on a mac, menu → darstellung → `menüleiste` (a section of its own) puts
kessler in the menu bar. the symbol is the app's icon made small: a rounded
box 16 points square with the k (the ケ in japanese) cut out of it — a square
as the icon is one, its corners rounded by 3½ points. `anzeige` (`[ui]
menubar`) is `an` or `aus`: `aus` (`aus`) puts nothing there and costs
nothing; `an` (`k`) shows the box with its k, playing or not. the picture is
made once and only again for another language, so it costs nothing while
music plays. the symbol is a template
image, so the menu bar colours it like its other ones. a plain click on it
plays or pauses; a right click, or a click with
ctrl, opens a menu 220 points wide: what is playing — its title, artist and
record a line each, wrapped to that width — then ←, `play` or `pause` and → as keys in one row, which keep the
menu open, under them `vol` and a slider (beside the keys in the same row),
which sets the volume as the rule in the header does; while an episode
plays, under them the sleep timer as a key of its own, saying what it says
in the episode's column — `einschlafen: aus`, `einschlafen in 12 min` —
each click a step on, the menu staying open (on linux a line of the panel's
menu); then `kessler zeigen`
and `kessler beenden`, lines between the three parts. each key shows a quiet plate
under the pointer, close around its word. above
the title stands the cover of what plays — a station's picture for a
station, an episode's own picture where it has one, else its podcast's,
which the system's own now-playing shows too — as `cover im menü` (`[ui]
menubar_cover`) says: `neben dem titel` (the default, beside the lines,
which wrap beside it, as tall as three of them), `über
dem titel` (as wide as the menu's text, above the lines, as far from the
menu's top as from its sides) or `aus`.
`gerastert` under it (`[ui] menubar_cover_screened`, on by default) screens
it as every picture in the window is — the method, tones and fineness of
darstellung → bilder, in the scheme's field, ink and second tone — or, off,
shows the cover itself. it is made on a thread of its own whenever the
track, the setting or the scheme changes, and written into the menu while it
is open.

on linux the same symbol stands in the panel's tray, under menu →
darstellung → `systemleiste`: as a status notifier item over d-bus, which
kde, xfce, cinnamon, lxqt and waybar draw themselves, and gnome with the
appindicator extension (ubuntu brings it). the same `an` and `aus`. a panel
has no template images, so `zeichenfarbe` (`[ui] menubar_ink`) chooses the
symbol's colour: `wie das system` (light while the system is dark, and when
it says nothing), `hell` or `dunkel`. a plain click plays or pauses, the
middle button brings the window to the front, and a right click opens the
panel's own menu: what plays, `play` or `pause`, `← voriger titel`, `→
nächster titel`, the volume (`vol 70`) with `+ lauter` and `− leiser` under
it, ten at a time (a panel's menu has no sliders), `kessler zeigen` (the
window to the front) and `beenden`.

a window too narrow for the columns side by side — under 640 points, more
with the queue open — shows one column at a time across its whole width, as
an ipod does, and slides to the next. under it a line names the columns on
either side, `← künstler:innen` at the left and `titel →` at the right; a
click goes there, as `h` and `l` do — into the fourth column too, whatever
it holds, and opening the information, the words or the log from there only
exchanges what the fourth holds. the header is the search field and `menü`
alone; under it what is playing, at label size and on three lines — title,
artist, album — then the timeline with its time at the left and the
transport keys at its right. what the status line says stands in the bottom
right-hand corner and what the library holds at the left, as on the compact
line. on a mac `menü` stands in the band's right-hand corner, so the search
field has its line to itself. with nothing beside it the field runs all the
way to the right edge, where `menü` and the keys end.

menu → kopfzeile → `was läuft` steps between the two places what is playing
can stand:

- `kompakt` — the switch (musik, radio, podcasts, ipod) and `menü` on a line
  of their own at the top right — in a mac's band, beside the window's
  buttons, and on linux in the same place, though there is no band — and
  everything else on one line under them: the search field at its left end,
  narrow until the pointer or the keyboard comes to it, when it opens to
  three quarters of four fifths of the way to 129 points past the width of
  the controls at the right — for typing as far as under the pointer —
  easing in and out over 0.6 s for the whole way, and under the pointer only
  once it has rested there a tenth of a second — and the names and the rule
  move along to make room; then the names at label size (while a station
  plays they take what the long rule would), the time and its rule, the
  transport keys, `vol`, the view and `menü`. a column with nothing written
  under it runs down into the foot's band, as far as the sides' margin, and
  slides back up when the library's figures or what the status line says
  come to stand under it. a list scrolled to its end stays where it stands
  as its column goes up and down (the queue slid down at every pause, as
  last.fm's count at the foot went away). what the status line says goes
  down to the bottom right-hand corner, in short (`last.fm 0:39`, `ipod 14
  titel · 4.8 gb frei`, `auswerfen`), in a band of its own, its baseline as
  far from the window's bottom edge as the sides are from its sides (24
  points; 28 before): the lists end a little earlier in this place, so the
  queue's fade is over before the status begins. at the other end of that
  line, flush left and there all the time, what the library holds: titles,
  albums, how long it plays, what it weighs. `bibliothekszahlen` under `was
  läuft` (`[ui] library_line`) shows it (`unten links`) or leaves it out
  (`aus`), here, under `super kompakt` and in a narrow window. what the
  status line says stands at the bottom right always (the key is no longer
  read). the keys are no longer read.
- `super kompakt` — the same line, set where the title bar was: on a mac in
  the band beside the window's three buttons (see above), elsewhere at the
  top with the flatter field, as far from the window's top edge as the sides
  are from its sides (under `kompakt` the line with the switch and `menü`
  likewise). its field is eight points narrower while shut (110 rather than
  118) and opens only nine points past the controls, and the time with its
  rule has a little more air either side, the same on both. a narrow window
  has its line of its own, as with `kompakt`.

a fresh install opens on `kompakt` (`[ui] playing_at`), and with the albums
in the track list (below, `[ui] albums_in_tracks`) —; a config that already
names either keeps what it says.

the side columns have their settings under menu → spalten: under
`informationen` each part of an artist's column can be left out — `bild`,
`steckbrief` (kind, origin, years, style, musicbrainz's note), `wikipedia`,
`bibliothek und radio`, `mitglieder, gruppen`, `ähnliche künstler:innen`
(`[ui] info_picture`, `info_facts`, `info_wikipedia`, `info_library`,
`info_ties`, `info_similar`, all on). while `bild` is shown, `bildbreite`
(`[ui] info_picture_wide`) says how wide the picture is: `fest`, 176 points
at most (the default), or `spaltenbreite`, as wide as the column — a
station's picture in the radio's column alike; `liedtext` sets the words at
content size or `klein` (`[ui] lyrics_size`) and whether timed words scroll
along (`mitlaufen`, `[ui] lyrics_follow`); `log` shows or leaves out each
kind (`[ui] log_hidden`, the kinds' words) and `exportieren` writes the
whole log as markdown, a day to a heading, to `kessler-log-<date>.md` on the
desktop; `andere künstler:innen` lists all, 10, 25 or 50 (`[ui]
others_shown`, 0 for all).

compilations: a record with no album artist, in one folder, by three or more
artists is a compilation. under one of its artists the track list shows it
whole — that artist's tracks as usual, everyone else's dimmed like the
queue's played rows, all under one heading that says `kompilation` under the
title and playable from anywhere in it. `sampler` under `alben` (`[ui]
compilations_whole`) switches back to the artist's own tracks only. only in
the shelf's own order, and not while searching. every track of a compilation
has its artist beside its title at label size, and the first column has a
`kompilationen` row under `zuletzt`, as itunes has: every compilation in the
library, as records under their headings (no one artist named under the
title), the album column naming them `verschiedene`. menu → spalten →
künstler:innen → `kompilationen-zeile` (`[ui] show_compilations`, on by
default) shows or hides it; not on the ipod.

repeat and shuffle stand beside the transport, or — with `zufall ·
wiederholen: über der warteschlange` — at the top of the queue's column right
after its name, as apple music has them, leaving the header ←, play and →. `r`
and `s` work either way. while a station plays neither is shown, in either
place, and the menu bar's menu has no arrows either.

what the status line says, in the header and at the foot, comes and goes with
a fade of an eighth of a second.

in every place it is the same line: click it to jump to what is playing, and a
long one scrolls under the pointer.

the selection sits on a **marker** — a highlighter stroke the width of its
column, not a widget surface, which is why it has no border. its air is the
same on both sides by construction: the row is allocated wider than its text
and the text is inset, rather than the marker being grown outwards. growing it
outwards puts the right-hand padding beyond the scroll area's clip, so it shows
on the left and vanishes on the right.

only the column with the keyboard has a marker.

**row air**: every row of the columns has four points more above and below
its words than the window works out — about the air the plate keeps left and
right of them. all of it goes to the plate; the gap between two plates
stays. the menu's rows and tabs have the same, so their plates are the
columns' plates.

**margins**: the columns, the masthead and the foot stand at most 24 points
in from the window's sides, so „menü"'s plate is as far from the right edge
as `super kompakt` sets it from the top on a mac; the words of two columns
stand two points less than twice as far apart as from the window's edge.

**arrows**: what is active has an arrow before it, pushing in from the left
over a sixth of a second and taking the name along — only on a row that
was on screen already: one coming back into sight, its view put away while
the radio or a podcast was started, or scrolled to, shows how things stand
at once — the artist and the
album of the track playing, the track playing in the queue, the option in
use in the menu's lists (the colour schemes, the cover's halftones and the
like), the menu's open tab and the playing track in the track list, between
its number and its title. `alle`, `zuletzt` and `kompilationen` have it
while they are the row chosen, and no figure beside them — one counted
tracks, the next artists (the row the window opens on wears no marker until
the first key or click): they are views rather than names, and chosen, the
view is open.

### one artist, one row

a library collected over years holds "boards of canada" and "boards of canada",
a name with a non-breaking space in it, and one with a stray zero-width
character some tag editor left behind. they are one artist, and every other
music player treats them as one.

kessler stores a **key** beside each track — case folded, accents off,
invisible characters dropped, runs of whitespace collapsed — and groups and
filters on that, never on the text as typed. the name shown is the spelling
most of the tracks use, with the alphabetically first as a tiebreak, so the
answer is the same on every run. albums are keyed the same way, on the pair
(album, artist), because two different *greatest hits* are two albums.

the spelling in the file is never touched. this only decides what belongs
together.

### covers

covers are pulled out of the tags once, during the scan, and written to
`artwork/` beside the database as tile-sized jpegs — one per album, not one per
track. a thousand albums is a few tens of megabytes there. on screen, only what
is visible is ever turned into a texture, on a thread of its own, so a long
list fills in as it scrolls instead of stopping the window; until a cover is
there its place is the blank tile. what is held at once stays under 48 mb
and under 64 covers, a few screenfuls; a cover in its own colours is held at
the size it is drawn at, not the 400 pixels it is stored at.

screened covers are dots of the ink on the ground. on a dark ground the dots
stand where the picture is light; on a light ground, where it is dark — so a
cover is never its own negative, whichever scheme is on. that goes for the
headings in the track list, the cover beside what is playing and the artist's
picture alike.

the cache is disposable: nothing in the database points at it, the file names
come from the album's own keys, and deleting the folder costs a rescan and
nothing else. an album with no embedded picture gets the same quiet tint the
search field uses, with its initial in it.

### the music, seen

a picture of the sound, in the search field, drawn out of a tap sitting in the audio path between the decoder and the
output, copying frames into a ring buffer as they go past. nothing is decoded
twice and nothing is guessed from the track's tags — what you see is what is
leaving the program. with nothing playing, the field is a field.

**menü → kopfzeile → `im suchfeld`**: `nichts` (the default on a first start
the scope is what kessler costs most while music plays; a config that has it
keeps it), `vektorskop`, or one of the quiet pictures (`[ui] field_picture`,
with `field_spectrum` on) that move only in the frames the time under the
title asks for anyway — four a second in front, one with `1 bild/s, mit der
uhr` — and ask for none of their own while a track plays, so they cost
nothing more than the field empty (on the air, where there is no time to
move on, once a second). all of them stand in the same coarse cells, the
field sixteen cells tall (a config naming it gets the scope): `schlange`, a
snake of dots that goes after a dot, eats it and grows, swerves on a loud
moment, and starts again small with the next track — hold `x` and the arrow
keys turn it (one step a tick, as it goes anyway; into a wall or into itself
and it starts again small; let go of `x` and it finds its own way);
`mandelbrot`, a zoom into the mandelbrot set towards a point where it
spirals, as deep as the track is far along — at the end about ten thousand
million times — in rings of tone round the set, from ink to none and back,
screened by blue noise in cells half as big as the others', the set itself
solid, worked out again only when the time under the title moves (on the air
one dive every ten minutes); `leben`, the game of life on the field, seeded
with the cover screened into dots (without one, at random), one generation a
tick, a glider dropped on a loud moment and new ones when it goes still;
`kugel`, a ball, lit from the upper left, rolling round the field a cell a
tick the way the logo of a dvd player waiting for a disc does, screened
another way at every wall it bounces off — ordered dots, lines across, down
and aslant, clustered dots, noise, a checkerboard. its edge is always inked,
a thin outline one cell wide. in the menu's head every one of these pictures
sits on a square plate the colour of the search field, with the same rounded
corners (the ball all of them), as it does in the field itself. paused, the
plate stays and shows the app icon's k (the ケ in japanese) instead, until
the music goes on. the k stands there whenever nothing plays — paused,
stopped or before the first track, whatever the field shows — and it is set
in the window's own typeface, at its width and its bold (a face without a
weight axis gives the icon's k). also while something plays and the field
shows no picture (`nichts`). `zarathustra`, nietzsche's „also sprach
zarathustra" filling the field in misaki's 8 × 8 dots, a dot a cell: three
lines edge to edge, justified (the space between words shared out, words
hyphenated by german rules of thumb — one consonant goes with the vowel
after it, of several the last does, ch, ck, sch, ph, th, qu and a stop
before r or l stay together — not by a dictionary; in the menu's head, where
a line holds only a few words, as a wall of text letter after letter to the
right edge, the words broken anywhere, without a hyphen), going up a row of
dots a tick (a line every two seconds; nine lines filling the screen saver)
— only while something plays, and on from where it stopped last time: the
place is kept as `book_at` in `[ui]` (a byte into the book), written every
half minute of reading. the text is the german second edition of 1893 as the
internet archive read it off its scans; the odd misread word remains.
umlauts are the small letters with two dots above; misaki has no capital
umlauts nor the low „, so kessler draws them in its manner. the row goes
round nichts → vektorskop → schlange → mandelbrot → leben → kugel →
zarathustra → kessler saga and back to nichts. how many steps a second the
quiet pictures take — all of them alike — is `bildrate`, set in straight
under `im suchfeld` in darstellung (`[ui] field_rate`; the saga alone had
its own): 1; 2; 4 (the default), on the clock's own quarter seconds, so no
frame more than an empty field; or 8. all of them on the quarter second's
grid the rule under the title is drawn on — a file that says 5 is read as 4,
one that says 6 as 8. the quiet pictures fill the field to its edge its
rounded corners cut off them as by a mask; the book keeps a dot's width of
air at each side, the same left and right (the right was a dot wider), and
each place that shows it — the field, the menu's head, the screen saver —
reads on from its own line's end. at 1 the time under the title moves on the
second too, and the window draws once a second. on a station the same steps,
which then ask for frames of their own.

`kessler saga`, a little figure walking in the middle of the field, fifteen
frames of 15 × 26 dots, a frame a tick, while things go by it right to left,
near ones faster than far ones. far off, half there in dots and at a quarter
of its pace: towers of capsules, domes, lattice masts, slabs of windows. at
half its pace: timber frames under gables, masts with a wire slung between
them, heaps of rubble, walls in steps, the arch of a ruin, a palisade — and
seldom something big: a stack of capsules round two shafts, a water tower, a
crane with its hook let down, a tower of roofs. on the ground at its own
pace stones, grit and tufts; now and then a block of rock or a post in front
of it at twice. the world is as busy as the music was when a thing came in
at the right edge — quiet, a few things; loud, many, and birds in the sky —
and one stretch of the walk in three is nearly empty, so the figure walks
through open country now and then; what has come in stays until it has gone
out at the left. the first time the saga is shown in a run of the program —
with the first track — a small „kessler" comes down from the top to the
middle of the field and stands there a moment before the walk begins, as the
maker's name does when a game starts on the grey handheld of 1989. then the
figure walks in from the left edge through the empty field to the middle,
where it stays, and only after that the country comes in from the right. how
quickly it walks is `bildrate` under `im suchfeld` in darstellung, as for
every quiet picture (see below). everything is drawn in the figure's own
dots, as many pixels to a dot as the field's height allows for the figure
(where the scene is taller, the row of air above goes first, then the row it
walks on — on a retina mac the field is 52 pixels, two a dot; the screen
saver draws it the same way, bigger) — and what goes by behind it stops a
cell short of it, so it always reads in front.

**the scope.** a vectorscope draws two signals
against each other, one across and one up, and lets the beam trace whatever
shape the pair makes — a studio's way of looking at a mix, and, drawn as lines
that glow and fade, the most musical picture there is: every shape in it is
the actual wave, not a reading of it.

the field holds one figure, wide, with every register inside it. seven
band-pass filters split what the card is playing; each register draws the left
channel of its band against the right channel a quarter of a wave later, which
turns a note into a ring — mono records included — and a noise into a tangle.
the registers are drawn *inside* each other, the lowest outermost and each one
above it a ring further in: a bass drum blooms at the rim, a voice knots the
middle, a cymbal throws a burst of short strokes at the centre, their lines
crossing wherever two registers are busy at once.

each register's *size* is measured against how loud it has been
over the last second, so every register fills its place when it is busy and
shrinks when it rests — which is the rhythm. its *brightness* is its share of
the music right now, lifted a little per octave because music loses energy
going up; without that, one low note drew every register at once, each scaled
up to fill its place from whatever leaked into it.

what is drawn is light rather than lines, the way a scope's phosphor works:
every sample leaves the same amount, spread along the stroke to the next, so
where the beam moves slowly the stroke is bright and where it jumps it is
faint. the light halves every thirtieth of a second, so a figure trails its
last few shapes as ghosts, and the grey is screened into dots at the screen's
own pixels with whichever screen the covers use. about a hundred thousand
multiplications a frame, and none while nothing plays or the field is in use.

**the field changes hands.** while the picture is drawing, the word `suchen`
fades out of it — `suchen` is a label, it names what the field is for, and
naming it is exactly what is not wanted while something else is using it. move
the pointer over the field, or put the keyboard in it, and the two trade
places: the word comes back and the picture goes, so the field is a field
again. a pointer over it is enough — no click needed, because somebody moving
the mouse there is already on their way to typing, and a field that only
clears itself once you have clicked is one you have to click twice.
words typed and left in the field keep it too, until it is empty again, so
the picture never runs over what is being searched for.

what is *typed* in the field never fades. that is not a label; it is what the
columns below are showing, and hiding it would be hiding a fact.

### the masthead

the top line is symmetrical on purpose. the search field is cut to exactly the
width of everything on the other side of it — `vol` and its rule, the view
switch, the device, `menü` — so the two ends of the line weigh the same. the
controls stand one gap apart, text to text, and `vol` a little closer to its
own rule than to anything else, because the two are one control.

### what it costs to leave open

standing still, nothing. the window draws only when something happens, so an
idle kessler with nothing playing uses no processor at all — which is the
number that matters for a program meant to stand open all day.

what costs is whatever moves, and not because moving it is expensive: the
picture behind the search field is one small texture. it is that moving *anything* in an interface of this kind
means redrawing the whole window — all four columns, every row of type — for
that one frame. so the frames themselves are what is rationed:

- **twenty a second** for the scope while the window has the keyboard — or
  twelve or eight, **menü → darstellung → im suchfeld → `bildrate`** (`[ui]
  scope_rate`), shown while the scope is on (on linux on arm, a raspberry pi
  most likely, it starts at twelve, and `bildrate` at thirty); each of its
  frames is the whole window redrawn, so this is most of what kessler costs
  while it is looked at. the light fades by the clock, not by the frame, so
  the afterglow lasts as long whatever the rate. the pointer and the keys
  get as many as `bildrate` allows. there is a fourth step, `4 bilder/s,
  belichtet` (`scope_rate = 4`): four pictures a second, each long-exposed —
  everything heard since the last one is laid down in twentieths of a
  second, as the twenty a second would have drawn it, on a plate whose light
  lasts longer (a fifth of a quarter of a second ago is still there), so
  each picture holds the strokes of the five it stands for. it moves in
  stills rather than running, and costs about a fifth of the twenty a
  second. the scope in the menu's head is exposed the same way. there is a
  fifth, `1 bild/s, mit der uhr` (`scope_rate = 1`): one picture a second,
  the whole second laid down at once, drawn in the frame the clock under the
  title draws anyway — and the rule then moves once a second too, so while
  music plays the window is drawn once a second in front as well, as it is
  in the background (about a tenth of what the twenty a second cost,
  measured on linux). the ring of sound kessler keeps for the pictures holds
  three seconds at cd rate for them.
- **one a second** while it does not: the scope fades out over half a second
  and stays out, and only the clock ticks over, on the second. it fades back
  when the window comes to the front again.
- **none at all** when nothing is moving.
- **one a second** while the window is minimized, covered, or put away with
  the red button — the music plays on and the queue moves on, but nothing is
  worked out for a picture nobody can see.

a pause longer than ten seconds ends on a freshly opened output, and an
output that goes away or changes while kessler holds it — the lid closed,
headphones taken off — is opened again where the track stood (play could be
pressed after a pause and nothing was heard). the sound card is let go of
when there is nothing to play. an open output never stops — with nothing to
play it sends silence to the card all day, and on a mac a running output
keeps the machine from going to sleep by itself. so the device is opened
when the first track starts, and closed again twenty seconds after the music
stops, or five minutes into a pause; pressing play after that opens it again
and carries on where it was. a side effect worth having: it opens on
whatever the system's output is *then*, so headphones plugged in while
nothing played are the ones it plays through.

the ipod watcher looks at what is mounted every two seconds, and only at the
names: looking inside a volume for an ipod wakes a sleeping external disk, so
that is done only when the list of names changes.

the spectrum behind both pictures is only computed when one of them is on
screen. it used to run on every frame that anything was playing, whether or
not either picture was switched on.

### the albums in the track list

**menü → spalten → alben → ansicht** switches between `eigene spalte` and
`mit titelliste kombiniert`. combined with the track list, the album column
goes, and each record stands as a heading over its own tracks — its cover,
its name, and under it the year and how many tracks (and whose, when the
list is not one artist's). the heading answers the hand as the album's row
does: a click takes all its tracks, a double-click puts the whole record,
and only it, at the end of the queue and plays it, a pull carries it, a
right-click opens the album's menu. the list is always in the shelf's own
order there — every record under its heading, in its own running order — so
there are no heads over the figures. a record is shelved under its album
artist and its earliest year, so one whose tracks name several artists or
disagree about the year stays under one heading (on the ipod's shelf it came
apart, and its count of records ran higher than the device column's): the
column is headed `alben`, like the other columns on the same line, with how
many records and tracks it holds at the right.

what belongs to the record moves up into its heading: of the columns switched
on under menu → spalten → titelliste, `jahr`, `genre` and `hinzugefügt`
are said once in the line under the record's name, and the rows keep only
`wertung` and `plays` from there. `nr` and `länge` have switches of their own
for this view (`[ui] merged_number`, `merged_length`, both on to begin with):
while it is chosen, `titelliste` shows these two in place of the
plain list's, and leaves out `album`, which is not shown at all — the name is
the heading. the menu only lists what the view in use shows.

**a column of its own for the right-hand side.** the combined view has one
name column fewer, and that room goes to the right: the ipod, the artist
information and the log get a column of their own beside the queue and the
lists, instead of lying over them. the queue and the lists stay on the left,
the rest on the right, and each side opens and closes by itself — `w` or `u`
on the left, `a`, `t` or the ipod on the right. with one side open it stands
where the fourth column always stood; with both, the second one takes its
width out of the track list. the row in this view is dealt out evenly — half each with two columns, a third with three, a quarter with four — and stays so as columns open and close (`[ui] merged_even`). dragging a divider ends that and the widths are yours, however columns open and close afterwards; a double click on a divider makes the row even again. the divider between the two moves the boundary
only, the way a tiling window manager does (`[ui] side_share`; unset, as wide
as the queue). in a window too narrow for five columns the right-hand side
lies over the queue, as it does in the other views.

**alle spalten nebeneinander** (`[ui] all_columns`, off by default, to try
out): the right-hand side is no longer one column but one for each of the
information, the lyrics, the other artists, the ipod and the log that is open,
in that order, side by side right of the queue or the lists. opening one
leaves the others open, and each slides in and out by itself (as every column
does, so a slow frame does not hold it back). every column on the row is then
the same width — the name columns, the track list, the queue and each of the
side, however many are open; widths dragged earlier do not count, and the
dividers cannot be dragged while it is on. it needs the same width as a column
of its own for the side, and then works in the view with both name columns
too; in a narrower window everything is as before, one at a time.

menu → künstler:innen → `zuletzt-zeile` (`[ui] show_recent`) shows the
`zuletzt` row or hides it; hidden, the lists lose their row of the newest
tracks too and the window opens on the whole library (the ipod keeps its
`zuletzt hinzugefügt` list). `zuletzt` shows 80 tracks at first, and
scrolled to the end of the track list it fetches 80 more, and again. the row
after it, `beim öffnen zeigen`, says where the window opens: on `zuletzt`
(the default) or on `alle` (`[ui] start_on_recent`; a row of its own set in
under `zuletzt-zeile` as `zuletzt hinzugefügt` / `ganze bibliothek` before).
under `zuletzt` the list is read as records too: the newest record first,
each one whole and in its own running order, until there are about as many
tracks as `zuletzt` shows.

with `mit titelliste kombiniert` chosen, the same menu section has, set in
one step under `ansicht`, the headings' cover: `cover` (an / aus — without,
a heading is its two lines of words; `[ui] heading_covers`) and
`covergrösse` (32 to 256 points — `[ui] heading_cover_size`; never wider
than the column). once the cover takes more than a third of the column, the
record's name and the line under it stand under the cover rather than beside
it. how it is drawn is `bilder`'s, below. after them, `titelzahl` (`[ui]
heading_count`, on) says whether a heading ends in how many tracks it has —
`2006 · 9 titel` or `2006`. nothing in the menu is set in more than one
step.

### pictures

**menu → darstellung → bilder** says how every picture is drawn: the covers
on the headings and beside what is playing, the artist's picture under
`informationen`, the turning iPod in the device column. `anzeige` —
`gerastert` (screened into dots in the ink, the default) or `im original`;
while screened, `raster` (its list stays open until the row is clicked
again, as the schemes' does, so one after the other can be tried; how the
dots are laid down: geordnet, halbton, streuung, atkinson, rauschen, and
blaues rauschen — blue noise, dots as scattered as the grain's but kept
apart, no clumps and no holes) and `feinheit` (from a mosaic to nearly the
picture back) — `[ui] cover_dither`, `dither_method`, `dither_fineness`.
under the rows stands an example, drawn as every picture is now and again at
once with each row changed: a ground running from dark to light for the
tones, a lit ball for a soft passage, a flat triangle and a flat square for
slanted and straight edges. it is drawn by kessler itself, not a file and
nobody's cover. in colour which shows under `im original`. also `töne`
(`[ui] dither_tones`, `2`, `3` or `4`; in four, the scheme's faint tone
stands under the accent for the lightest parts — the ink, the accent, the
faint tone and the ground): in three, a second colour stands between the ink
and the ground, where the picture is half light: the scheme's accent. where
the third would be the colour the picture is drawn in — the record playing
is in the accent — it goes half way to the field instead. every method works
in three tones, deciding between the two levels a pixel lies between. the
colours between the ink and the ground are each at their share of the way
from the ground's lightness to the ink's — a half in three, a third and two
thirds in four — taken from the scheme's own colours: the accent where it is
about as near as any, otherwise the scheme's colour nearest that lightness
(a colourful one before a grey), brought the rest of the way keeping its
hue. on a light ground they are also taken down to a quiet colourfulness, so
a full red under dark ink no longer outshouts the picture. the screen saver
keeps its own `verfahren`.

### allgemein

the menu's first tab. `sprache`; `mausrad` (`[ui] scroll_speed`): how far a
notch of a mouse wheel scrolls — `normal` five rows on linux, about as far
as a browser goes there, and 40 points on a mac as before; `langsam` two
thirds of that, `schnell` two thirds more. on linux a trackpad's or a
trackpoint's own distances go twice as far as they come, by the same steps;
on a mac they are left as the system sends them. a notch is spread over a
few frames; what is left of it goes only where the wheel was turned — moved
more than 24 points off from there, the rest is dropped, so a column the
pointer moves on to does not scroll by itself. rows scrolled out of sight
are not laid out or drawn at all, so a long list scrolls as lightly as a
short one. and under `funktionen` what kessler does at all, each on or off:

- `musik` (`[ui] music`, on). off, kessler is a radio or a podcast player
  and nothing else: `musik` leaves the masthead's switch, the radio, the
  podcasts or the ipod stand in its place, whichever is on, the folders are
  not read at the start, nothing of the library is held (no artists, records
  or tracks read, and the library's figures not shown over the radio either)
  and the loudness of its files is not measured — read and measured again
  once it is on — and a track of the library that plays stops — and leaves
  the header: the queue keeps it. nothing starts one either — not play,
  `space`, `n`, `p` nor the keyboard's media keys — and nothing of it goes
  to an ipod (on it what is there stays to be looked at and played). with
  everything off the window shows a drawing by felix h-w of someone stacking
  pots (in the scheme's receded accent, a little over half the room high,
  left out in a window too low for it) and under it `alles ist
  ausgeschaltet`, `musik, radio oder podcasts unter menü → allgemein
  einschalten` under it (a click opens the menu on that tab) and under that
  `oder 1 für musik, 2 für radio, 3 für podcasts`, the digits on small
  square plates, their middle on the middle of the words' x-height, a little
  more air beside them than between words (square and set so). the hint is
  in the small type, and the line at the top — search, what plays, the keys,
  the volume — and the library's figures at the foot are gone: there is
  nothing to play. only the menu's `•••` stays in its corner, and where the
  library's figures stood, bottom left, which kessler this is — name,
  version and commit, as the menu's head has them; gone again as soon as
  anything is switched on.
- `radio` (`[ui] radio`). off, the radio's tab is gone too.
- `ipod` (`[ui] ipod`, on). off, kessler does not look for an ipod, lets go
  of one plugged in, and the ipod's tab is gone.
- `podcasts` (`[ui] podcasts`, off). on, the masthead's switch offers
  `podcasts`.

`einstellungen zurücksetzen` sets every setting of the menu back to how
kessler first opens — appearance, columns and their widths, pictures,
typeface, the views' settings. the music's folders, last.fm, the ipod, the
player and the parts switched on above stay, and so does where the window
stands. the first click says `wirklich?` at the row's end; a second within
four seconds does it.

the screen saver's section stands under it.

### screen saver

**menü → allgemein → bildschirmschoner**. its first row turns it on or off;
only while it is on do the rows below it show, set in one step (`jetzt
zeigen` is always there, for a look before turning it on). only the rows
that touch what it shows: the vectorscope has them all, the words of the
song only `grösse`, a quiet picture none beyond `startet` and `zeigt`.
`startet` (`[ui] saver_after`, minutes, 0 is off; turned on it starts at 5):
after 1, 5 or 20 minutes without a key, a click, a movement or the wheel,
the vectorscope comes up over the whole window as one figure, every register
nested inside the others, screened in the accent — more waves of each note
than the field draws, and a longer afterglow. what is playing stands under
it.

`wann` (`saver_silent`): `nur bei musik`, the default, comes only while
music plays and goes when it stops. `auch ohne musik` comes when nothing
plays too, and then draws vibrations of its own: three made-up tones two
registers apart, each a ring that turns from a line to a circle and back as
its right channel drifts against its left, each coming and going at its own
slow pace, so the nested rings never repeat. nothing is played — the tones
are numbers handed to the scope, not sound. `jetzt zeigen`, or the key `b`,
brings it up by hand either way; with nothing playing and `nur bei musik` it shows the
scope's still figure.

`zeigt` (`saver_picture`): `vektorskop`, the default; also the search
field's quiet pictures — `schlange`, `mandelbrot`, `leben`, `kugel` —
filling the window in coarse cells in the accent, 48 rows of them, moving on
the same ticks as in the field (four a second, one on the air) and drawn
only on those, also with nothing playing; or `liedtext`, the words of the
song filling the window. timed words show the line being sung in the very
middle of the window (before the first line, the first, not yet coloured),
in the accent, the four before and after it in the middle tone, fading
towards the edges; every line is the same size and weight, so only the
colour changes as the song goes on. where the words carry moments of their
own (enhanced lrc, `<01:23.45>word`), the line being sung is coloured word
by word as far as it has been sung, the rest of it in the ink; the lyrics
column shows such lines without the moments. words without moments stand a
page at a time, as many lines as fit, the page turning as the song goes on
(on the radio, the first page). `grösse` sets how big the words are (5, 7,
10 or 14 % of the window's height). a song without words says so in the
middle of the window instead, in the words' size: `lade liedtext …` while
they are being looked for, `kein liedtext gefunden`, or `ohne worte` for an
instrumental. the vectorscope is there only while nothing plays. the words
draw only when they change: when a line begins and when a page turns; and
while the saver is up the time under the title asks for no frames of its
own.

`farben` (`saver_colours`, 0–2): `akzent`, the default, draws every dot in
the scheme's accent. `wandernd` draws them in one colour that wanders slowly
through the scheme's own — the accent, the ink, the receded accent, the middle
tone and a halftone's colour where the scheme has one; any too close to the
ground are left out — holding each a few seconds and easing into the next.
`verlauf` uses two of them at once, half a round apart, as a gradient across
the figure that turns once in a minute and a half while both wander. a scheme
with few colours of its own wanders through few.

`grösse` (`saver_size`, 0–3): `klein`, `mittel` and `gross` take half, 70 %
and 86 % of the window's shorter side; `ganzes fenster` makes the figure as
big as the longer side, so it runs off every edge and fills the window.
`punkte` (`saver_dots`, 1–3 device pixels a dot), `nachleuchten`
(`saver_glow`, 0–2), `deckkraft` (`saver_strength`, 0–2), `verfahren`
(`saver_dither_method`, its own screen, „geordnet" by default — an
error-diffusing screen reshuffles every dot each frame and shimmers). the
exposure follows the music: whatever is lit is brought to the chosen
strength and eased, so a quiet ambient piece draws as clearly as a loud
record. it runs at sixty frames a second while the window has focus, laid on
whole device pixels. the touch that ends it is used up by it; it never comes
while the search field is being typed in or a transfer is running.

### language

**menü → darstellung → sprache** (`[ui] language`): `system`, `deutsch`,
`english` or `日本語`. a fresh config follows the system — german on a german
locale, japanese on a japanese one, english everywhere else. the whole window is translated; the changelog
(`CHANGELOG.md`) and the command line stay german.

in japanese kessler is ケスラー — in the window's title and at the foot of
the menu — and the icon's k is a ケ, drawn from the bundled m plus 1 code
at its bold weight, since the window's own faces have no katakana. a word with no japanese
yet is said in english.

in the source every word the window says is written in german and wrapped:
`tr!("warteschlange")`, `tr!("noch {}", n)` (bare `{}` only, filled in turn).
the english lives in `src/lang/en/*.rs` and the japanese in
`src/lang/ja/*.rs`, both keyed by the german; the two tables must hold the
same words. a german word
with two meanings carries the second after a bar — `tr!("titel|name")` is
„titel" / „title", where `tr!("titel")` is „titel" / „tracks". words that
are also config values (column names, scheme names) are looked up where they
are shown, with `lang::t(..)`. tests read the source and fail on a `tr!` with
no english or no japanese, on an english with no german left to translate,
on a german given two englishes (or two japaneses) and on mismatched `{}`.

### size

**menü → darstellung → grösse** (`[ui] zoom`): the whole window, bigger or
smaller than the system makes it — a rule with a notch for each of 75 %, 80
%, 90 %, `wie das system`, 110 %, 125 %, 150 % and 175 %. pull it to a
notch; the window takes the new size when you let go. `cmd` + `+` and `cmd`
+ `−` (`ctrl` on linux) go a notch up or down from the keyboard, `cmd` + `0`
back to the system's size. the change is made behind a blink: the window
fades to its ground in 60 ms, takes the new size there, and fades back in
over 140 ms, so the faces being laid out afresh and the grid's jump are not
seen. the config keeps the factor (`1.0` is the system's size); any value to
2 written there by hand is taken, and anything else is the system's. for the
machine whose system gets it wrong: a small laptop under x11 with no scale
set has one worked out from the panel's millimetres, and kessler comes out
half again as big as everything else.

### frame rate

**menü → allgemein → sparsam → bildrate** (`[ui] frame_cap`): the most
frames a second the window draws — `höchstens 60 bilder/s`, `30`, or `wie
der bildschirm` (`0`, the default), as many as the screen shows. a mac with
a 120 hz screen otherwise draws every move of the pointer twice as often as
anybody can follow. nothing the hand does is lost: what came in meanwhile is
taken by the next frame. the scope and the clock ask for fewer frames than
this anyway and are not slowed.

### colour schemes

a change of colours — another scheme, light to dark, the ipod's shelf —
fades over a quarter of a second rather than snapping, so going dark in a dim
room is not a flash.

**colour on the plates and in the heads**: every scheme shows its colour
behind the words — the marked row, the search field, the rule and the hover
each take a colour at the lightness they had, a grey plate the hue of the
scheme's most colourful plate or of its accent — and in the columns' heads,
which take a colour of their own, as dark as the second tone, so they stand
apart from the lists (a scheme that names its heads' colour keeps it). the
words keep the scheme's own tones. the search field is the exception: it
takes the ground's hue, halfway to the ground's lightness, nearly without
colour, so it does not read as a button. the key is no longer read.

every scheme is held to three measures, as wcag reckons contrast: the marked
row's plate at least 1.4:1 against the ground (it goes further towards the
ink where the scheme's marker sits on the ground's own lightness), the third
tone at least 2.2:1 against the ground, and whatever is small on the marked
plate — a track's number, `flac`, its length, the playing row's receded
accent — at least 3:1 against the plate: such a word keeps its hue and is
made only as much lighter or darker as it must. `cargo test scheme_report --
--ignored --nocapture` prints the figures.

**every scheme as a film**: with `alt` held, `über` shows `farbschemen als
gif` and `farbschemen als mp4`; `alt` + `shift` + `g` and `alt` + `shift` +
`m` do the same from anywhere. the menu closes, and the view as it stands is
shown in every scheme in turn — each light, then dark, then the next — with
its name and half in the status line, without the usual fade. each is caught
as it stands and one file goes to the desktop, or the home folder where
there is none: `kessler-farbschemen-<date>.gif` or `.mp4`, 0.8 seconds a
picture. the gif is at most 1280 pixels wide and goes round for ever; the
mp4 keeps the window's own size (up to 2880 pixels) and every colour, made
by ffmpeg in h.264 (`crf` 16), and needs ffmpeg — without it kessler says so
before it starts. the mp4's pictures go to a temporary folder as they are
caught, so memory stays small. `esc`, or the same keys again, stops it; the
keyboard does nothing else meanwhile. your own scheme is in it only while it
is the one in use. the scheme you had comes back after, and the config is
never touched.

**menü → darstellung**, two rows `farben` chooses the scheme (`[ui]
ground`), and `hell, dunkel` whether it is light, dark or `system` (`[ui]
brightness`) — every scheme comes in a light and a dark half. under `system`
the half follows the operating system and switches while the window is open
when you switch the system; the row says which half that currently is. on
linux the window system does not pass that on, so kessler asks the desktop
portal (`org.freedesktop.appearance color-scheme`), and gnome's `gsettings`
where there is none. `shift` + `t` goes from the half that shows to the
other and fixes it there. `shift` + `f` goes on to the next scheme in the
menu's order, round again at the end (`alt` + `shift` + `f` goes back one),
in the half that shows — light stays light, dark stays dark; the status line
names it. the config keeps the half that is painted; one with no
`brightness` is read off the scheme — `systemfarben` becomes the native pair
(macos on a mac, gnome on linux) under `system`, any other name that half,
fixed.

the settled schemes:
`macos`, built from the system's own values so the window sits in a mac
desktop rather than on it; `gnome`, the same for gnome — adwaita's window
ground, its text at the opacities libadwaita sets labels at, and gnome's
blue; `solarisiert`, `nord`, `dracula` and `sublime` — the schemes people
already read code in all day, the last being mariana, the one sublime text
opens with, with its caret's orange as the accent (nord's light half is its
snow storm, dracula's is alucard); and `ipod classic`, which comes from an
object rather than a screen: the grey of the panel and the ink on it. two
colours, no third; dark, the panel unlit. and `soulseek`, a homage: the
file-sharing client as it looked on windows 2000 — the grey of its buttons as
the ground, black text, the white of its lists behind the picked row and the
navy of its title bar as the accent, taken off a screenshot of it.

the list stands in groups, two columns side by side so all of them fit
without scrolling: **system** (macos, gnome), **klassiker** (solarisiert,
nord, nord frost, dracula, sublime, soulseek), **geräte** (ipod classic,
hu-card, pc engine, pc-fx, fm towns, pc-98, x68000), **eigene welten** (公園,
paysage d'hiver, coco壱番屋, runebending, rubrik, wilder mann, ochsenblut,
lehmgefach, rgb, zeitvogel) and **selbst gemacht** (eigenes) — this last
only while `alt` (⌥) is held, or while it is the scheme in use. `shift` +
`f` leafs through them in that order (`theme::Theme::GROUPS`), past
`eigenes` unless it is in use.

and louder ones, each with its other half made by turning its machine's
colours round. most come from machines that never left japan:

| scheme | from |
| --- | --- |
| `nord frost` | nord's frost blue as the ground, polar night on it, aurora's red |
| `hu-card` | (`hudson` is read as it) hudson soft's blue, white on it, and the bee's yellow for the heads, the figures, the rule and what plays; by day the yellow paled as the ground and full behind the picked row, the logo's blue beside the names, a hucard's red for the heads |
| `pc engine` | the white console, navy words, and its colours in the words: the blue of its name beside them, the box's orange for heads, figures and the marker, the logo's red for what plays; dark on its navy |
| `pc-fx` | white, and the faces of its cube in the words: the spiral's purple for the heads, pink beside the names, yellow figures and the yellow marker, blue for what plays |
| `pc-98` | the pc-9801 at night, kept apart from pc-fx: navy, a blue white, and its sixteen colours in the words — cyan beside the names, green figures, yellow heads, magenta for what plays; light on a pale cyan |
| `x68000` | sharp's black tower and the red of its x |
| `fm towns` | townsos: white windows, the pink title bar, the red menu bar |
| `公園` | a park in japan in autumn, with a shōwa playground's kitsch wet gravel, names in the yellow of the bar's joint, a tiled mint beside them, salmon figures, tangerine heads, the red bar as the marker, the covers in the yellow — the marked one in the blue of the pole; by day a warm gravel, green, salmon and the pole's blue for the heads|
| `coco壱番屋` | the curry house's sign: its yellow, paled as the ground, the curry's brown as the words, dark gold figures, curry-orange heads, a curry red for what plays; dark, the roux as the ground, rice-white words, golden rice beside them, terracotta figures, the sign's yellow for the heads|
| `runebending` | (the colourful `runebender` beside it is gone, as are `famicom`, `disk system` and `virtual boy` — a config naming one opens on `pc engine`) runebender the other way round, for the type: the grounds a quiet warm grey, everything behind the words a shade of it, and the colour all in the words — names in ink, what stands beside them in the grid's green, the figures in its orange, what plays in the comb's violet; dark, the toolbar's black with the green and orange lifted and pink for what plays|
| `rubrik` | rubrication, as old books were printed: black and red on a verdigris paper — names in black, what stands beside them in the rubricator's red, the figures in a slate blue-grey, what plays in blue, nothing behind the words but the paper a shade darker; dark, by lamplight, cream on a dark paper with a warmer red and a lighter blue|
| `wilder mann` | gruvbox's colours, every kind of thing in one of its own, so a head, a name, what stands beside it, a figure and what plays are told apart before they are read: column heads in yellow, names in gruvbox's cream, what stands beside them in blue, figures in grey, what plays in red; by day on gruvbox's light ground in its deeper colours. named after the „wilder mann", the brace figure of a hessian half-timbered house|
| `ochsenblut` | the same idea in the ox-blood red the timbers were painted: heads in red, what stands beside the names in orange, what plays in green, the marked row a dark red; by day on pale plaster|
| `lehmgefach` | the clay of the panels between the timbers: a cream ground, names in the timbers' dark, heads in burnt clay, what stands beside the names in moss green, figures in a dull violet, what plays in blue; by night the clay dark and every colour lifted|
| `rgb` | only the colours whose channels are all full or off: white names on black, green beside them, cyan figures, magenta heads, red for what plays, pure blue behind the picked row, in the search field and in the rule (the hover a blue at half, the one exception). on white only blue, red and magenta read, with yellow, cyan and green behind the words. louder behind the words than the contrast test lets any other scheme be|
| `zeitvogel` | the bird that lives in time, from a toy's tile with a cuckoo clock printed on it, lime and lavender tiles round it: by day the lime as the ground, dark green words, the roof's teal beside the names, heads in violet, coral for what plays, the lavender tile behind the picked row; by night the lavender darkened to a deep violet, the clock's green lightened beside the names, the roof's teal for the heads, the lime behind the picked row. no other scheme has either ground.|
| `eigenes` | one's own, made in the menu: under „farben" four colours — ground, text, accent, heads — each with three rules (lightness, colourfulness, hue, in oklab). the rest is worked out from them: the second and third tones step from the text towards the ground, the marker, search field, rule and hover step from the ground towards the text, the faint accent goes halfway to the ground. the other half turns every lightness over and keeps the hues. kept in `[ui] custom_colours` as a code like `k1-282828-ebdbb2-fb4934-fabd2f`; under the three rules, „hex" takes the colour typed in (`#fb4934`, `fb4934` or `#f43`, enter takes it); a click on „code" copies it, „code einfügen…" takes somebody else's (with or without `k1`, `#` or spaces)|
| `paysage d'hiver` | grey and nothing else, after the band's page: black and the greys of snow at night, what plays in pure white; light, the static grey of its header with black words |

every one of them passes the same contrast test as the rest: where a
machine's own colour could not carry text at 7:1, it was moved just far
enough that it could.

a fresh install opens on the native pair under `system`. a config file that
still names one of the schemes taken out (`papier`, `tinte`, `monokai`, `gruvbox`, `riso`, `blaupause`)
opens on `macos hell` or `macos dunkel`, whichever is the same brightness.

the macos schemes take their marker from the *unemphasised* selection colour —
the quiet grey a list uses when its window is not frontmost — rather than the
loud accent blue, because that one is a fill and this design has none.

each is fitted to the same five roles rather than copied wholesale: one primary
value, one secondary, one tertiary, one accent, one marker. a test checks every
one of them for contrast — primary text at 7:1 against its ground, secondary at
3:1, the accent at 3.5:1 — so a scheme cannot be added that merely looks right
in the one screenshot it was picked from.

### columns

the track list shows the number, the title and the length. everything else —
`jahr`, `genre`, `wertung`, `plays`, `hinzugefügt` — is off until you switch
it on under **menü → spalten**. a column you read past on every row costs
more than the fact it carries, unless you asked for it. `wertung` shows a
rating as stars, `★★★` for three, and is 64 points wide.

figures stand right-aligned, so their digits line up down the column —
except the track number, which starts the row and stands at its left edge,
under the cover with the records in the list, and the title follows it at
half the air the other columns keep. `schriftgrösse` under `länge` (`[ui]
small_length`) sets the length at the label's size, as the times beside
„gerade gehört" in the radio, its ink on the middle of the title's; `gross`
(the default) as big as the title. in the track list and in the one with the
records in it.

a title too long for its room runs on over the empty left part of the column
after it, up to ten points before that column's own figures, and fades out
there.

only the columns actually shown can be sorted by. there is no hidden sort
order, and no head offering to sort by something you cannot see.

**too many for the width**: the title keeps at least 140 points, and the
columns that would take it are left out, in this order: `genre`, `album`,
`hinzugefügt`, `plays`, `wertung`, `jahr`, `länge`, `nr`. they stay switched
on and come back as the column grows.

**the cursor** in the track list is only marked while the list has the keys:
looking at an artist or an album, its first track does not stand there in
bold as if it had been chosen. tracks picked by hand — a run, a record by its
heading, a second one with `cmd` — stay marked wherever the keys are.

**what format a file is** is said beside its title, small and quiet, and only
when there is something to say: `flac`, `wav`, `aiff`. the ordinary lossy ones
stay silent — `mp3` beside nine hundred titles is a column of noise that says
only "this is a music library", while `flac` beside one of them answers the
question you actually have. it is the big file, the good copy, and the one no
ipod will play, so when the same record is on the disk twice the mark says at
a glance which row is which.

read off the extension, because that is all there is to read: a `.m4a` may
hold apple lossless or plain aac and the name does not say, so it stays quiet
rather than guessing. the mark's room comes off the title's on every row,
whether or not it is used, so the columns to its right do not step sideways as
you scroll past a lossless file.

the artists column can carry a track count per artist — **menü → spalten →
titelzahl bei interpreten**. it is off by default: a figure you look for now
and then does not earn a place you read past on every row.

the top line holds the search field on the left and, on the right, `vol`, the
view switch, the device when one is plugged in, and `menü`. under it, what is
playing, with the time at the far right, and under that the progress rule.

the transport stands in the top line beside `vol`: back, play, on, repeat,
shuffle. while a station plays, back and on are not shown — stations are
changed from the list, or with the keys — and play stands where on stood,
its word moved on so it stands as far from `vol` as everything else on the
line stands apart — measured on the wider of `play` and `pause`, so the key
does not move when it changes its word. the keys stand in two groups: back,
play and on, and repeat and shuffle a little way off, as far from on as one
control on the line stands from the next. when repeat and shuffle stand over
the queue (`modes_in_queue`), they stay there while a station plays. the
marks are the typeface's own: `←` and `→` skip, `uni21BB.alt` — a
face's second drawing of `↻`, two arrows going round — repeats, `①` when
it is one track rather than the queue, and last, `⇵` turned on its side
(`⇆`) shuffles. no character reaches that second drawing and egui has no
stylistic sets, so the marks are read out of the font file by glyph name and
filled here, a step above the label size, so that their stems are the words'
stems (`src/gui/glyphs.rs`). a face without them gets lines drawn at the
same weight instead. on is the ink, off is faint (on was the accent and off
the ink before).

the keys are set the way the header's own controls are: a mark on the ground,
the faint plate behind it only under the pointer, and the ink for what is on.
the play key is a word rather than a glyph, and it says what pressing it will
do: `pause`, lit in the accent while the music plays, and `play` in the ink
while it is held or nothing has started. it is as wide as the longer of the
two, so nothing beside it moves when it changes its word. (the keys alone made
more sense.)

the menu stands on the window's own grid: its top edge where the column
heads begin, its right edge where their rows end, its rows on the same pitch
as a list row at its roomiest. it scrolls when it outgrows the window, and
dims the list behind it with black rather than with the scheme's own ground
— veiling a light scheme with its own light ground would brighten the list
rather than step it back. the veil alone says that the sheet is in front of
the list — a drop shadow would not suit the window's terminal plainness —
and so does the key sheet `?` shows. that sheet stands in the middle of the window as
the menu does, and has `schliessen` in its top right-hand corner as the menu
has. its keys stand on small plates, the figures and tall letters in their
middle, as much air above as below (the plate kept room for the tails of g
and p, and they stood high in it). what is the same in the music, the radio
and the podcasts — moving, picking, the views, the search — stands once,
under `bewegen`, `ansichten` and `suchen`; under `radio` and `podcasts` only
what is theirs alone. its search field, `tippen zum suchen`, stands on the
search field's own ground, as tall and with the same air as the masthead's.
the menu a right-click opens keeps its shadow — nothing is veiled behind it,
and without one it would melt into the list it stands on. with last.fm
connected, a track's menu — in the track list, the queue, a list — ends in
`bei last.fm lieben`. a column divider under the pointer shows the system's
column-resize cursor for as long as the pointer stays. menu →
künstler:innen → `anzahl oben rechts` (`[ui] artist_total`, off by default)
writes how many artists there are at the head of their column; the figure
does nothing and does not light under the pointer. in the radio and the
podcasts what a row offers under the pointer goes while the list is
scrolled, as the hover does. a feed is given up only when nothing has come
for twenty seconds, three minutes at most, and `bilder neu laden` takes the
picture the feed names now.

in a window wide enough for two columns it takes two: everything that *does*
something on the left, and everything that is there to be *looked up* on the
right — the keys, the version, what changed. otherwise the same rows in one
column, in the same order.

the left-hand column is ordered the way the window is: after `bibliothek`,
`darstellung` (language, size, colours, typeface, and on a mac the menu
bar's symbol), `kopfzeile` (what stands in the line along the top: `was
läuft`, `zufall · wiederholen`, `im suchfeld` and its `bildrate`), which are
about the whole of it, come the three columns of the window itself, in the
order they stand in — `künstler:innen`, `alben`, `titelliste` — each holding
the settings that change that one column and nothing else. a heading saying
`spalten` alone left it to the reader to work out which of the window's
columns had columns of its own. with the albums in the track list (`mit
titelliste kombiniert`) there is no track list of its own: its columns are
set under `alben`, set in under the view, and `titelliste` is not there.

**idtags ergänzen** is the menu's section for filling in what the files do
not say: `cover` → `in den dateien suchen` (look again for pictures beside
the music, for the records that had none last time) and the years and genres
from musicbrainz below. a cover from the cover sources is asked for one
record at a time, `cover suchen…` on its right-click; most look for a cover
now and then, not for all of them at once.

**years from musicbrainz.** menu → idtags ergänzen → `jahre` → `fehlende suchen…`
goes through every record that has no year and asks musicbrainz for the date
it first came out — the record's own year, not a reissue's. an answer is only
taken when it is plainly about the same record: title and artist match once
case, accents and punctuation are set aside, and musicbrainz is at least 90 %
sure of its own match. then the year is written into the files' tags and read
back in, exactly as the tag editor does it. one question a second, as
musicbrainz asks of everybody; the row says how far it has got, and a click on
it stops. a year that is already there is never touched.

**genres from musicbrainz.** `genres` → `fehlende suchen…` does the same for
the records none of whose tracks has a genre: the record is found the same
way, then asked for its genres — musicbrainz's list of genres, not the tags
anybody may add — and the one most people gave is written, in musicbrainz's
own small letters. two questions a record, a second apart. only one of the
two runs at a time; the other row waits.

the order each column was left in — the artists, the records, the track list
— is kept in the settings, so the window opens the way it was closed.

playback runs from one track into the next without a gap: the next file is
decoded and laid behind the one playing, so the hand-over happens inside the
sound card's own queue. for mp3, flac and wav that is the whole of it. aac
files carry a little silence at both ends that the encoder could not avoid,
and a note of how much; kessler reads the note — itunes' `iTunSMPB`, or the
track's edit list — and cuts it off, so two halves of one piece meet sample
for sample. menu → `lückenlos` turns the hand-over off.

every file is **read ahead**: decoded on a thread of its own about three
seconds before it is heard, so the sound card only ever takes finished
samples from a store. a machine busy with something else — rendering, an
image being generated, memory paging to disk — then costs some of that
store, not a gap in the music.

**levelling.** menu → wiedergabe → `lautstärke angleichen`: `aus`, `pro
titel` (every track equally loud — for shuffle) or `pro album` (every record
equally loud, its tracks as they were mixed). it is replaygain 2.0, as
foobar2000 and swinsian use it: a file's own replaygain tags are taken as
they are; everything else is decoded once and measured the way ebu r 128
does — k-weighted, 400 ms blocks, gated — and brought to −18 lufs, never
turned up past its peak. a record's figure comes from its tracks weighted by
length. the answers are kept in the database, so each file is measured once,
and again only when the file changes. while levelling is on, new music is
measured in the background after every scan; the status line and the row
under the setting count while it runs, and a click on the row stops it.
up to four records are measured at once, half as many as the machine has
cores; while music plays only
one goes on, slowly, so the sound never waits for the disk.
switching between the three is heard at once, mid-track.

there is no permanent strip of key hints along the bottom: those are worth
reading twice and then never again. `m` opens a menu instead, which holds the
library folders, a rescan, the colour scheme, the columns, gapless playback,
last.fm, the full key list and the version. the track and album counts live
there too, rather than on a line you read past all day.

what the menu does **not** hold is a row for every fact about the program. the
typeface, the fallback face, whether audio is available and whether the media
keys are in the build are all answers that never change and are nearly always
the same one — so each of those rows appears only when the answer is the other
one: no `fonts` folder, no fallback face, no output device, no media keys. a
line that always reads the same is a line nobody reads.

keys: `j k h l` or the arrow keys to move, `Enter` to play, `Space` to
pause, `n`/`p` to skip, `Shift`+`←`/`→` (or `Shift`+`h`/`l`) to seek ten
seconds, `u` for the lists (`w` swaps the queue back), `Shift+T` for light
or dark, `m` for the menu, `q` to close. menu → allgemein → `tasten` shows
all of them, as `?` does. `[ui] ground` sets the default ground. über also
has the project on github a click away, and thanks mattis, matthias, leo,
niklas and rade (matthias). under them, `gehört`: how long kessler has
played, all told, and in brackets after it, in the smaller size, the music,
the radio and the podcasts as bare numbers in the same unit — `312 std
(musik 280, radio 20, podcast 12)`, minutes under an hour. where the row
cannot hold it, the three are set in under it, a row each (in the content
size in brackets before, which ran off the row). under the pointer the total
says `auf null setzen`: a first click asks `sicher? noch einmal klicken`, a
second within five seconds sets all three to nought. each is counted while
it really sounds, at most two seconds a frame, and kept in `gehoert.toml`
beside the settings every minute and on closing. the music from before the
count began is taken, once, from the library: every track's plays times its
length; the radio and the podcasts count from then on. where the build has
muto, über names it too, `muto — eine schrift von rade matic`, one line; a
click opens radematic.com. `neue version` asks github, only when clicked,
for the newest release and compares its number with the one built: `das ist
die neueste`, or `1.4.140 ist da — öffnen`, which opens the release's page;
nothing is downloaded. where github does not show the releases, the row
says `nicht zu sehen` (`KESSLER_UPDATE_ENDPOINT` for a stand-in). it names the
developer, `entwickler` → `windpark soft`; a click opens its page,
windparkbooks.de/soft. it folds out `lizenzen`: first
kessler's own, `gpl-3.0` (gnu general public license, version 3 or later; a
click opens it — see `LICENSE`), then felix's drawings under their own
terms, cc by-nc-nd 4.0 (see `LICENSE-drawing.md`; it is a quotation,
ascribed to people of the hfg ulm or of the bauhaus), the faces egui carries
itself (ubuntu light under the ubuntu font licence, noto emoji under the
ofl, hack under mit), the faces kessler carries (asap and m plus 1 code
under the sil open font license 1.1, muto only within kessler — a click
shows a face's licence file), misaki (free, num kadoma), symphonia
(mpl-2.0), sqlite (public domain) and the rest of the rust libraries (mit,
apache-2.0, bsd, zlib).

the head of the queue counts what is still to come. under the pointer its name
turns into `schliessen`, which puts the queue away like every other column
(`w` brings it back), and the count into `leeren`, a word of its own at the
right: a click there empties the queue. every word in a head that does
something wears the same light plate under the pointer and stands in the ink.
both words come up, faint and without a plate, as soon as the pointer is
anywhere on the head; only the one under the pointer lights, and only a click
on `schliessen` itself closes the column — a click beside it does nothing. the panels with
`schliessen` at the right of their head — the transfer, the tag editor, a
question — close only from that word too.

**a double-click never empties the queue.** a track, a record (in the album
column or as a heading in the track list), a name, `alle`, `zuletzt` or a
list: whatever is double-clicked — or chosen with `enter` — goes on at the
end of the queue and plays from its first track, and the column scrolls down
to it. what was queued before stays where it was, above what is playing;
tracks that had not had their turn yet can be dragged back down below it.

the menu marks things the way the window does: in a list that has been
opened (farben, verfahren) the one in use is bold, and a switch that is on
says `an` in bold — under either plate as well, as a chosen row is in the
columns (before, the one in use had the arrow the playing track has and both
were in the accent). a row that hangs off the one above it is set in by a
step, as an opened list is: `beim start einlesen` under `einlesen`, `dabei`
and `und an music.app` under `musik`, whatever belongs to the view under
`ansicht`, `raster` and `feinheit` under the `darstellung` of `bilder`,
`flac umwandeln` under `überspielen`. under `über`, its rows are followed by
the motto, centred in the column, — „»warum großschreiben, wenn man nicht
groß sprechen kann?«", the reason everything here is lower case, a quotation
ascribed to people of the hfg ulm or of the bauhaus — and the room the
taller tabs leave holds a drawing by felix h-w, as large as fits, at the
foot of the column, its face in the middle of it, in the receded accent, as
every drawing; with `lizenzen` folded out it keeps that size and goes on
under the list (`src/gui/drawing.png`, made from the scan by
`bundle/make-drawing.py`).

### last.fm

menu → last.fm → `konto` → `verbinden…`; with no key yet it asks for one
first. scrobbling needs an api account of your own, which last.fm hands out
to anyone with a login in a minute — `bei last.fm beantragen…`, under the two
fields, opens the page. with a key and no account the section is `verbinden…`
and, set in under it, `schlüssel` → `ändern…`. paste the *api key* and the
*shared secret* into the two fields (tab between them; what is pasted around
them, a label or a line break, is dropped), press enter, and agree in the
browser that opens. from then on kessler scrobbles, says what is playing if
you want it to, and asks last.fm for covers and artists too.
`kessler lastfm setup --key … --secret …` does the same from a terminal.

connected, the section names the account and offers three rows under it:
`profil im browser…` opens its page on last.fm, `anderes konto verbinden…`
signs it out and starts connecting again — last.fm asks the browser who is
logged in there, so sign in with the other account on the page that opens —
and `abmelden` forgets the session. the key and the secret stay, and
scrobbles still waiting to be sent go out with whichever account is
connected next. below them, `„läuft gerade“ senden` (`[lastfm] now_playing`,
on by default) tells last.fm what is playing as it starts, so the profile
shows it live; it takes effect at the next start. while scrobbles wait to be
sent, `wartend` says how many.

### the radio

switched on under allgemein (`radio`; the tab), the masthead has the word
`radio` beside `ipod` — in the switch `musik radio ipod` next to `menü`,
which chooses what the columns read: `musik` always, `radio` once it is
switched on, `ipod` while one is plugged in; with only one of them on there
is nothing to choose and no switch — but that one's word stands there,
underlined, for two and a half seconds after arriving in it (the window
opening, a view switched off, `c` and a digit, a view switched on from
everything off) and then fades; under the pointer it comes back. the search
field names what it looks for in every view (see the radio's). all the words
stand alike, in the second tone and the regular cut, each as wide as it is,
18 points apart; the one open is underlined by a line as heavy as the face's
strokes, a little lower than an underline usually sits, its ends only just
rounded (in muto heavier, to go with its medium labels, a point and a half
lower, its ends square), and the line wanders to the word chosen over 0.26
seconds — only then: drawing the window narrower moves it with the words, at
once. under the pointer a word is in the ink, on its plate. an ipod plugged
in stands in the accent for 2.5 seconds, fading back, so it is not missed.
while tracks are carried from the library `ipod` widens into a place to put
them down, tinted with the accent and more so under them. this is the only
way. in „super kompakt" the shut search field is as wide as `stationen
suchen` in every view once the radio is offered, so switching does not move
the line. the menu's trigger at the top right is three dots, its plate only
under the pointer, square and as tall as a control's plate. where the
trigger stands up in the band — „kompakt" and a narrow window on a mac — the
switch stands there too, just left of it, rather than on the line below.
every control in the window (the search field's word, the offers in column
heads, links, the device column's commands, the words at the end of a row,
the column arrows, `menü`) keeps the rule the masthead has; news keeps the
accent. every control in the masthead keeps one rule: at rest in the
scheme's second tone, as `menü` and `vol` are, under the pointer in the ink
on its plate, and on — playing, lit, chosen — in the accent; so `←`, `play`,
`→`, repeat and shuffle (no longer faint when off), and the switch with its
marks between the words. `strich` and `punkt` are `trenner` with a `|` or a
`·` between the words instead of the `/`. (a config naming one reads as
`trenner`.) tracks carried from the library still open `ipod` as a place to
put them down. a click turns the columns over to the radio: the stations
where the artists stand, and under them, in the same column, `über den
sender`; then `gerade gehört` with `gemerkte lieder` under it; then whatever
of the side is open — `i` for `informationen` (only while the station names
an artist), `y` for the lyrics, `t` for the log. the albums and the queue
have nothing to say about a station and wait behind it as they were; `w` and
`u` do nothing while the radio is open. the column of stations stays as the
artists' does, without `schliessen`; `musik` in the switch gives the library
back, `ipod` the device. in a narrow window the stations, the station and
what was heard share one column. `↑` `↓` move among the stations and `enter`
plays one, as in any column. the tab also has `logos der stationen laden`
(`[ui] radio_logos`, on), `beispielsender hinzufügen` (with ⌥ held), and,
with last.fm connected, the two rows below. switched off, a station on the
air stops. the stations kept are there — none to begin with: the list starts
empty and says how to fill it, `noch keine stationen — oben im suchfeld
einen namen suchen oder eine stream-adresse einfügen`, and the column beside
says `nichts ausgewählt`. `beispielsender hinzufügen` puts five examples in
at the next opening (`radio.toml`'s `examples`): nts 1 and nts 2, montez
press radio, kzsu stanford and cashmere radio (kept where it was taken in
already). all but nts are looked up in the directory (their streams'
addresses are the directory's to know), at every opening and whenever a
search gets an answer, until they are found. `vergessen` takes any of them
out, and it stays out: `radio.toml` remembers in `offered` which were
offered already.

while the radio is open the search field looks for stations rather than
music: once the typing stops, kessler asks [radio
browser](https://www.radio-browser.info), an open directory that needs no
key, and the column shows what came back (`radio · suche`). they come best
first: kessler asks for a hundred and ranks them by how well the name
answers what was typed (all of it, its start, the start of one of its words)
together with how much the station is liked by the directory's own figures —
its votes and how often it was played over the last day (`clickcount`,
`clicktrend`), on a log scale, so a well-liked station beats a nameless copy
that happens to have the exact name. letters found only inside a word
(„montents" for „nts") come after all the rest. one name listed several
times is shown once, its best entry, and forty are shown. the search reads a
land out of what is typed — by its name or its adjective, in german, english
or japanese („jazz japan", „japanischer jazz", „日本のジャズ", „deutscher hip
hop", „south korea indie") — and then asks only for stations there; the
other words are looked for in the names and in the genres the directory's
users gave the stations, a genre counting a little below a word of the name.
a land alone lists its best-liked stations. the genres are english, so „ジャズ"
finds jazz only where a station's name says it. under the pointer a row
offers `merken` at its end, which keeps the station, or `vergessen` for one
already kept; a click anywhere else on the row plays it — the pointer turns
into a hand over the word, as over every word at a row's end that does
something. stations that send hls are found and played too: the playlist's
best version of sound alone up to 320 kbit/s is taken (or its audio
rendition), and its pieces are fetched one after the other, a live one three
pieces behind its end and asked again as it grows. pieces in mpeg-ts are
taken apart to the aac or mp3 inside, plain aac or mp3 pieces go as they
are, and where a station puts what plays in id3 tags (in the pieces or
beside them) it shows as an icecast station's title does. encrypted pieces
and fragmented mp4 are not played yet: the station says so. that a station
comes as hls stands among its facts in the station's information, between
its format and its bitrate (`aac · hls · 128 kbit/s`), as the directory says
it or as its stream came; the lists of stations found leave it out.

beside each station found (and each like station) stands, small and set
solid rather than tracked, what the directory says of it: its first genre,
its land as two letters, its format (`mp3`, `aac`, `ogg` — at one bitrate aac
sounds a good deal better) and its bitrate in kbit/s. a station the
directory's last look found silent says `stumm` where its genre would stand,
its name faint as a played row's, and comes after every station that
answered, however well its name matches. menu → radio → `angaben bei suchtreffern` (`[ui]
radio_found`) sets them `rechts daneben` (`"rechts"`, the default), small at
the row's end — „jazz · jp · aac 128", after `gemerkt` where it is kept — or
`in spalten` (`"spalten"`), each in a column of its own under the heads
`genre`, `land`, `format` and `kbit/s`, the figures flush right, and a
station kept in bold, so stations can be compared
down the list; where the column is too narrow for the name to keep 100
points, it falls back to the row's end. `aus` (`"aus"`) shows the name
alone. under the pointer the word that does something takes their place.
the stations kept show none of it.

**a station by its address.** typed or pasted into the search field, the
address of a stream (`https://…/live.mp3`) or of a `.pls` or `.m3u` naming
one is not looked for: it is the one station shown, to be played and kept
like any found one — for a station the directory does not have, or has
wrong. it is called after its server („stream.example.org") until its
stream says its own name as it begins (`icy-name`); then it takes that one,
in the list and in `radio.toml`.

**stations in and out.** menu → radio → `gemerkte stationen` → `als m3u
speichern` writes the stations kept to the desktop (the home folder where
there is none) as `kessler-stationen-<date>.m3u`, each with its name — any
player reads it, and it shows in the file manager. set in under it,
`stationen aus datei` → `hinzufügen…` asks for an `.m3u` or a `.pls` and keeps
every station it names (`#EXTINF` or `TitleN=` for the name, else the
server), the ones kept already once; the status line says how many were new.

**about the station.** the column `sender` gives, beside its picture, its
land and language, its genres, its format and bitrate (the stream's own once
it plays, else the directory's), how many of the directory's users voted for
it, and — where the directory's last look found it silent — `zuletzt stumm,
laut verzeichnis`.

**one word for many, one for one.** the list is `stationen` — `stationen ·
suche`, `ähnliche stationen`, `stationen suchen`, `beispielsender` — and one
of them is a `sender`: the column about it, `der sender sagt …`.

a station has no length and nowhere to jump to: where the time would stand
there is a dot, and `live` after it. tuning in is one movement after the
other, each eased: a small ball comes up (half a second), pushes out to the
right into the little rule (0.9 s), a piece travels to and fro on it — at
least one turn of two seconds, and until the station is there — then the
rule draws in again (1.2 s), stays still a moment (0.5 s), and `live` fades
in beside the ball, which then stands still. with a square-ended face (muto)
it begins with a square instead of the ball, which pushes out into the
square rule and draws back in to the square; once the station is there, the
square rounds into the ball while `live` fades in. drawn out again for
another station, the ball squares off first. while the rule is at work the
line before it stays `<station> verbinde`, its dots going as everywhere
something is under way, and only when the station is there — as the ball
stands, or the square rounds into it — does it say what the station plays,
fading in with `live`. every change of the station's line cross-fades in
0.3 s, and the dot and `live` glide to their new place. paused, the rule draws in and
the ball shrinks away; with nothing on the air the radio view has no rule at
all. a station that answers at once, and a change to another, go
through it the same way. once the station plays, nothing in the rule moves,
and the window, which has no position to move along for a station, is drawn
only when the station says something new (every two seconds at most). `live` comes after the dot,
not first. tuning in to another station, the dot stays for the first second,
and only a station that takes longer gets the rule back. paused, the dot is
still and faint. pause lets go of the station (a stream held still only
falls behind the air), and play tunes in again. `←` and `→` (and the media
keys) go through the stations kept. playing a track from the library ends
the radio; the queue waits, unmarked, while a station plays.

a station whose connection goes quiet without being closed — the network
gone, a router asleep — is heard as silence, the window answers as ever, and
after fifteen seconds without a byte kessler tunes in again, a few times.

most stations say what they play: kessler asks for it the shoutcast/icecast
way (`Icy-MetaData: 1`) and takes the lines out of the stream before the
decoder sees them. the masthead then shows the title, the artist and, faint,
the station; a station that names its shows rather than songs shows the
show. `gerade gehört` lists what the station on the air has said, newest
first with the time it came (on 24 hours) (forty at most, forgotten when
another station plays). with the radio open, `informationen` follows the
artist playing now — when the station names one as „artist - title" — and a
click on an older line shows that artist instead until the next song comes.
a station that sends nothing of what it plays — many do not — says so under
`gerade gehört` (`der sender sagt nicht, was läuft`; with nothing on the
air, `es läuft gerade kein sender`, in `informationen` too), and the log
says it for each station when it answers.

the stations' column is headed `stationen` (`sender` heads the column beside
it). the stations kept can be dragged into another order: a line shows where
the one carried will go. `KESSLER_RADIO_BROWSER` points the directory elsewhere, for testing.

a station's picture is its own from the directory when that is one that
can be shown and at least 48 pixels; otherwise the icon its own page names
(the home-screen icon, then the picture it shows when shared, then its
favicon, kept under `radio-logos/site-<md5>`), then its wikipedia article's
picture. while nts plays, the show on now brings its own picture, shown in
the header's cover and under `informationen`.

the radio's columns share the row evenly, as the library's do. the gaps
between them are handles: drag one to widen the column on its left,
double-click it to share the row out evenly again (`[ui]
radio_stations_share`, `radio_side_share`). the row is: the
stations; what is known about the station; the programme and what was heard;
and the side — `informationen`, the lyrics or the log, taking turns as in the
library, sliding in and out. the second and third stand while the radio is
open and share what the side leaves, alike unless the gap between them is
dragged (`[ui] radio_page_share`, the station's share, cleared by a double
click with the others). paused, the dot draws itself out to the little rule
again, and stays one while play tunes the station in anew. while a station is
tuned in, the line thickens to about half the dot as the piece travels, and
drawing in to the dot takes 1.1 seconds. `live` itself stands only while the
station plays: not while it is tuned in, nor paused; it fades in as the little
rule draws itself in to the dot, drawn along behind it.

`gerade gehört` has the newest at the top. in the radio view the library's
figures stay away from the foot, and while a station plays the foot says where
its song stands with last.fm, by the radio's rule: `last.fm noch 0:12` for the
first thirty seconds, then `zählt beim nächsten lied` for ten seconds, then
nothing; the song already under way when the station came on counts down four
minutes instead, and `zählt nicht` stays for one without an artist.
`entfernen` takes a station or a kept song out.

an artist's column says when and on which station they
were last heard on the radio (`im radio`: „heute · nts 1"), under the
library's figures, or under `zuletzt gehört` for somebody the library does
not have. kessler keeps it, one line per artist and at most two thousand,
in `radio-heard.toml` beside the settings — written only when a station
names somebody, and read only when an artist's column asks.

each view — the music, the radio, the podcasts, the ipod — keeps its own
side (back from the podcasts it stands at once as back from the radio, and
does not slide in again): whatever was open beside it when the switch went
elsewhere — `informationen`, the log, the lyrics, `andere künstler:innen`,
the equalizer — is open again when the switch comes back, whichever way it
went round. the first time, the radio has `informationen` open (or shut, as
it was last left, `[ui] radio_info`) and the podcasts and the ipod have
nothing beside them.

`informationen` opens with the radio and is only about whom the station
plays; while it names nobody, it shows the show on now with its picture
where the station publishes a programme (nts), else says that nobody is
named. a right click on a station offers `abspielen`, `ähnliche stationen`
and `hinzufügen` or `entfernen`. `ähnliche stationen` lists eight stations
like it in place of those kept, headed `ähnlich wie …`, which offers
`schliessen`; escape goes back too. nobody publishes which stations the same
people listen to — radio browser counts votes and plays per station, last.fm
knows no stations — so „like" is what the directory's users said of them:
asked for by its first three genres, and ranked by how many of its genres
they share, then by its language and land, then by how liked they are;
without genres, by its language and land alone. the side slides in and out
as the library's does.

what is known about a station stands in the second column (always): the
station on the air, else the one under the keyboard. its links — the
wikipedia article and its own page — stand one under the other under `mehr
lesen`, as an artist's do. the column is headed `sender`, and the station's
picture is small with its name beside it in the larger, bold face, as a
record's title stands by its cover, and land, genre and bitrate under the
name. playing another station puts away what was asked about the one before.
in the narrow window, which has no room for the column, `über den sender`
shows it at the foot of the stations. under nts's show on now,
`informationen` also gives the programme's sentence about it; it runs as an
artist's column does — the show's name, its picture under it, the sentence,
then where else it is to be found (its soundcloud, its youtube, from the
links in the programme's description) under `mehr lesen`. lines that only
name such a place are left out of the sentence, and with no address to them
they are not shown at all. on the radio the search field says `stationen
suchen`, under the podcasts `podcasts suchen`, and the library `musik
suchen` (`search library` in english); shut on the compact line it is as
wide as the widest of the words it may say, with the same air on both sides,
in every view. in the narrow window the radio keeps its own strip: the
stations with what was heard, and the side sliding in beside them while it
is open.

where a station logs its songs in spinitron (kzsu, so far), what it logged
stands in for the title its stream does not send: the newest of its
now-playing page, asked every 30 seconds while it plays
(`KESSLER_SPINITRON_ENDPOINT` points elsewhere for testing). the log says
whether spinitron answered. kzsu is known by its stream, its page or its
name; when the widget names nothing, spinitron's own page of the station
(`spinitron.com/KZSU/`, `KESSLER_SPINITRON_PAGE`) is asked instead.

a click on the station in the header goes to the radio from the library,
with the station under the keyboard; a click on a track from the library
playing goes back from the radio to the library, to the track.

the keyboard's row among the stations wears the dark marker as a row in the
library does; a click anywhere outside the stations' column lets go of it
(the station playing keeps its arrow), and the arrow keys or a click on a
station take it up again.

links, here and under `mehr lesen` in `informationen`, stand after a `›` and
are cut short with `…` when long; a page's address loses what follows `?`.

`i` on the radio shows `informationen` about whom the station names; while
it names nobody, the station itself at length — its picture, the show on
now, the facts, the whole article and its links.

where a station publishes its programme, `programm` stands over `gerade
gehört`, drawn as the queue is: the shows that went by while it was listened
to faint, the one on now in the accent with the time it has left (`noch 42
min`), and the next ones with the time they begin. so far that is nts, both
channels, from the programme its own site uses (`nts.live/api/v2/live`; the
programme is kept while the station plays, so the head names the same show in
the library; the log says which was found;
`KESSLER_MONTEZ_ENDPOINT` and `KESSLER_MONTEZ_SCHEDULE` for stand-ins), asked
when the station is tuned in, as the shows change, and every quarter of an
hour; a stream can only say what plays now, never what comes after. while nts
sends no title in its stream, the masthead shows the show on now.

`über den sender`, under the stations, is the station on the air, or the one
under the keyboard: its picture with its name beside it, and on one line under
the name what is known of it — country, language, genre, bitrate — then a few
sentences, then its links, `wikipedia` and its page, on one line. its picture
is the directory's, or, when there is none or it is not one that can be shown,
its wikipedia article's. what is known comes from the directory and from what
the station says about itself as its stream begins (the
shoutcast/icecast `icy-description`, `icy-genre`, `icy-url` and `icy-br`,
shown as `bitrate`; servers' placeholders such as „unspecified description"
are left out), and the station's wikipedia article, `aus wikipedia`, in the
window's language or, without one, in english or whichever there is. the
article is found by the station's page rather than its name — wikidata keeps
every station's official website — so „nts.live" finds nts radio and not
something else called nts. it is asked once and kept in `radio-about/` beside
the config; nothing found is asked again after thirty days. without an article
the station's own sentence stands there instead, `vom sender selbst`. either
is cut to its first sentences.

a station whose address is a `.pls` or `.m3u` playlist is played
from the first stream it names (playlists naming playlists are followed three
deep). a stream that breaks off is tuned in again, up to four times in a row,
after 2, 4, 8 and 16 seconds (`verbinde…` meanwhile, its dots going); one that holds for
thirty seconds earns its tries back. under the pointer a line under `gerade
gehört` offers `merken` in place of its time: the song goes to `gemerkte
lieder`, under it in the column, and `vergessen` takes it out again. with a
last.fm account connected, the tab `radio` has two more rows: `zu last.fm
scrobbeln` (`[ui] radio_scrobble`, on by default) sends what a station names
as „artist - title" like any track — playing now when it starts, and listened
to after four minutes, or when the next comes if it was heard from its start
for thirty seconds (its length is unknown, so last.fm's „half of it" cannot be
asked) — and `gemerkte lieder bei last.fm lieben` (`[ui] radio_love`, off by
default) loves a song when it is kept. the lyrics column shows the words of
what the station plays (not timed: where the song began is only roughly
known). a station found in the directory brings its picture, which is fetched
once into `radio-logos/` beside the config and shown in the station's column
and to the system (now playing, the menu bar).

outside the window, what a station plays is said like a track: the menu bar
and its menu, the system's now-playing panel and mpris (with the station's
picture), the screen saver and, under linux,
the notification. a station that cannot be reached says so in the status line
(`radio: … ist gerade nicht zu erreichen`); what exactly went wrong goes to
the log, under `radio`.

the stations kept are in `radio.toml` beside the config, one `[[station]]`
with `name`, `url` and perhaps `favicon` each, and the kept songs as
`[[song]]`; it can be edited by hand. streams and the
directory are reached through `curl`, as everything else in kessler is.
nothing of the radio is read, held or fetched before it is opened, and what
it held is let go of when it closes.

### the equalizer

`z` opens the equalizer in the side, in the music, the radio and the
podcasts alike — it sits where everything goes on its way to the card, so
one setting holds for all three, and the scope draws what is heard. `z`
again, or `schliessen` in its head, puts it away; the sound stays as set.

ten bands an octave apart, 31 hz to 16 khz, each a peaking filter an
octave wide, up to 12 db up or down. they stand up as the timeline lies
down: a thin rule each, thickening under the pointer, the part from nought
to the setting filled in the accent, the knob only under the hand, the value
on a badge (`+3,5 db`) while it is over or held. drag a band, click where it
should be, turn the wheel over it (half a decibel a notch), or double-click
it flat. behind them the curve all ten make together, drawn from the same
filters that play — what is heard, not only what is set; with the bands an
octave wide their neighbours overlap, so the curve can stand a little above
or between the knobs.

over the bands: `an` and `aus` — off, every sample goes through untouched —
and ready-made settings, `flach`, `mehr bass`, `weniger bass`, `mehr höhen`,
`stimme` (for speech: less rumble, more presence) and `loudness`, then
`eigene 1` and `eigene 2`, one's own (`[ui] eq_own_1`, `eq_own_2`). the one
that holds is underlined, in the ink, as the menu's tabs along the top are.
an own one is opened with a click and from then on keeps the bands as they
move — nothing to save (`[ui] eq_slot`). a ready-made setting chosen lets go
of it. touching a band switches the equalizer on.

`z` or a right click on the volume rule, or on the word `vol` beside it,
opens and closes the column (the right click on the word). the curve stays
inside the field: bands raised side by side add up past the top of the
scale; up to ten decibels it is drawn true, past that it leans into the
field's edge and never reaches it, rather than being cut off or running over
the words above. switched off, the bands stand flat and the curve is a
straight line — the setting is kept and comes back when it is switched on
again; a band touched while off starts from flat; near the top, a band's
badge stands under its knob.

the names under the bands (`31` … `16k`) lie on their side, turned a quarter
clockwise and read from the top down, where the column is too narrow for
them standing.

**no clipping.** whatever the curve raises, the whole is first lowered by the
curve's highest point, so a raised band cannot push the sound past full scale
and break up; under the bands it says by how much (`um 6 db leiser, damit
nichts übersteuert`). raising the bass makes the rest quieter rather than the
bass louder — that is the honest version of the same curve.

**no clicks.** a change walks to the new setting over 60 ms, the filters
worked out again every 32 frames on the way, and a seek forgets what they were
ringing with. with every band at nought and the walk over, nothing is done.

kept in `[ui]`: `eq_on`, and `eq_gains`, ten numbers in decibels from 31 hz
up (`"5.0 4.0 2.0 0.0 -1.0 -1.0 0.0 1.5 3.0 3.5"`).

### the lyrics

`y` puts the words of the song into the side, where the artist's column goes,
and takes them away again. they are the words of what is playing, so they can
be read along; with nothing playing, of the track under the cursor. the head
says `liedtext` (under the pointer `schliessen`, which closes the column), the
title stands under it, wrapped where it is long, the artist under that, and
over the words where they come from.

they are looked for in two places. first in the file itself: an id3 `USLT`
frame, or a `TXXX` frame named `LYRICS`, `UNSYNCEDLYRICS` or `USLT` (which is
what ffmpeg writes), in other formats a field of any of those names. then at
[lrclib](https://lrclib.net), an open collection that needs no key: kessler
searches it by artist and title and takes the first answer whose length is
within four seconds of the file's, timed words before plain ones. what lrclib
said — the words, that the song is an instrumental (`ohne worte`), or that it
had nothing (`kein liedtext gefunden`) — is kept in `lyrics/` beside the
library database, one file a song, so it is asked once. `neu suchen` at the
foot forgets it and asks again. if lrclib cannot be reached the column says
so and asks again the next time the song is shown.

timed words follow the music: the line being sung is in the accent and the
column keeps it in the middle, and a hand that scrolls away to read ahead is
left alone until the next line begins.

### the artist

`a` (or a right click on a name → „über künstler:in") gives the fourth column to
whoever is being looked at: the name picked in the first column, else the
artist of the track under the cursor — the track's own artist, so a
compilation shows who sings — else of what is playing. it follows as you move.
the head says `informationen` (under the pointer `schliessen`), and the name
stands under it, wrapped where it is long.

a right click on a name, a track, a record, a row of the queue or of an open
list → **`über künstler:in`** opens it on that artist, whoever the cursor is
on. it stays there until you pick a name, move the cursor or the music moves
on, and then follows again.

what it shows: a picture, screened like the covers; the kind (person,
group), where they are from and when they were active; their style; a
paragraph about them; your own note; who they are like — the ones in your
library a click away; and links to read further (wikipedia, discogs,
bandcamp, their site, last.fm, musicbrainz, and your own). the prose in the
information columns — the paragraph about an artist, your note, what
wikipedia or the station says about a station, a show's description — stands
with a little more air between its lines than a row has (1.32 times its
size; and there only), and so do a station's facts and the small notes in
those columns, such as `nichts gefunden — …`.

the first time an artist is shown, the sources are asked, once, on a thread
of their own: **musicbrainz** for who it is and the links (the name has to
match, alias included), **wikipedia** through the wikidata link musicbrainz
gives — never by name, because „coil" by name is a spring — in the window's
language first, **deezer** for a square picture when the name is exactly
theirs, and **last.fm**, when a key is set up, for tags, similar artists and
a biography where wikipedia has none. what they said is kept in
`artists/<key>.toml` and `.jpg` beside the library database, and read from
there from then on — the column works without a network. `neu suchen` asks
again, replacing what was found and nothing else.
while they are being asked, the column says so under the picture —
`lade informationen`, its dots counting up from one to three and round
again.

the paragraph is kept in **every language the window speaks** — german,
english and japanese — where wikipedia has it, so
switching the window's language switches the paragraph too, without asking
again. where the window's language is missing, another is shown with its
language beside the source — `aus wikipedia (de)`. an artist looked up before
a language was asked for is asked again by itself, once, when the column shows it in a
language it has no paragraph in (`artists/<key>.toml` remembers the languages
asked as `asked`). while it is asked, the paragraph in the other language
stays away and only `lade informationen…` stands there; when none is found, it
comes back. the source stands over its paragraph, the way every other heading
in the column does — `aus wikipedia` over the text, `aus musicbrainz` over
musicbrainz's own short note.

every search says in the **log** what it brought — `boards of canada: text
(de, en) · bild · 2 mitglieder · 8 ähnliche` — or that nothing was found.

**menu → bibliothek → über künstler:innen → `fehlende suchen…`** does this for every
artist in the library the sources have not been asked about, in the
background, at musicbrainz's pace (a second or two an artist, some seconds
for a group with members), so the column already has them when it is
opened. **`alle neu suchen…`** asks again about everyone — the oldest
answers first, so a run stopped halfway has renewed what most needed it;
what you wrote yourself stays. the row and the status line count while a
run goes; a click stops it, and what was found stays; the log says when it
started and how it ended. not at the same time as the search for years or
genres, which asks musicbrainz too.

a name in the column that is **not** in your library — a similar artist, a
member, a group — opens its own page in the same column on a click, looked
up the way anyone else is, with `← <the one before>` at the top to go back.
under the pointer `kopieren` stands at the right of the row: a click there
copies the name instead, for a search elsewhere.

`bearbeiten…` opens a form of five fields — from, active, style, link, note —
for what the sources do not know or get wrong. what is written there wins
over what was found wherever both exist, and a new search never touches it.
`bild aus datei…`, in the form, chooses the picture by hand; a search then leaves it alone.
tab moves between fields, enter saves, shift-enter makes a new line in the
note, escape leaves.

under the paragraph, **in deiner bibliothek**: the library's own figures for
them — records, tracks and how long they play, how often they were played,
since when they are here and when last heard. then, as on discogs, **who
is in it** for a group — the members, current first, with their years and
the other groups each of them is also in — and **which groups** a person
plays in. musicbrainz says so; the first six members are asked about one by
one, at its pace, so a group takes some seconds longer the first time.
names that are in your library are a click away. where nothing linked bandcamp
or discogs, the column offers a search there instead.

**menu → bibliothek → über künstler:innen → exportieren → `als markdown…`** writes everything gathered, one page an
artist, into a folder `kessler künstler:innen` wherever you choose — for a
notes program, or to keep. the facts go in yaml front matter (name, kind,
origin, active, style, tags, musicbrainz id), the rest as the column shows
it, with the picture beside the page. reading such pages back in is not
built yet.

### other artists

**`o`** (for others) gives the fourth column to the **other artists**: who the
artists in your library keep naming as similar and who is not in it yet, the
most named first. on the right, how many of your artists name them; under
each, which ones, the likest first. it is not a recommendation from outside:
it is counted from your own library, from what the artist column has kept
about each artist. someone named by only one of them is that artist's taste,
not the library's, and is left out.

a click on a name opens their page in the artist column, looked up like anyone
else, with `← andere künstler:innen` at the top to go back. `o` again, or
`schliessen`, closes the column.

at the top of the column, `verzweigt zeigen` (`[ui] others_tree`; `als liste
zeigen` goes back) sets each of them as a tree in braces: their name, a
brace round up to four of your artists who name them — in the ink where they
find them very like (a measure of a half or more), in the faint tone where
less, in between without a measure — and beside each of those a brace round
up to three others they name who are missing too, the most named first. so
one name leads to the next: every name in the tree opens their page, and
under the pointer stands on the plate a row has there. the braces are drawn
in the manner of muto's ⎧ ⎨ ⎩ (square ends, round shoulders, a point in the
middle), which has the pieces but none to fill between them; round one name
alone, a short stroke. each level stands as wide as its widest name among
the trees shown, at most a third of the column, so the braces sit close by
the names, left, and in the same place in every tree. each brace is in the
tone of the names it holds — the first in the likest one's, the second in
the others'.

the count uses last.fm's **similar artists with their measure** (up to 30
each, `artist.getSimilar`), which every search keeps beside the five names the
page shows. artists searched by an older version have only those five names
and no measure; they count all the same, and the foot of the column says how
many there are — `alle neu suchen…` brings them up to date. the foot also says
how many of your artists the count was made from; `fehlende suchen…` there
asks about the rest. nothing is asked of the network for the count itself,
and it is made again whenever something new is found about anyone.

### podcasts

a view of their own as the radio is: with `podcasts` on under allgemein
(`[ui] podcasts`), the masthead's switch has `podcasts` beside `musik` and
`radio`. an episode whose server does not connect within fifteen seconds, or
sends nothing for a minute, is given up on and says so. nothing is read or
fetched before the view opens, and all of it lives in `src/podcast/` and its
columns, so it can go again as easily as it came. choosing `musik` or
`radio` puts it away; an episode playing plays on.

- **the podcasts** — the first column: `alle`, then `offen` with how many:
  every episode begun and not heard to the end, under its own head
  `angefangen` at the top, then every one not begun that came out since its
  podcast was followed, under `heute`, `diese woche`, `älter`; never one
  archived — what is heard to the end leaves by itself. then `geladen` with
  what the episodes fetched take on the disk beside it — while archived
  episodes are hidden, what of it is theirs too: `56.4 mb archiviert` where
  it is all theirs, `84 mb · 56 archiviert` where part, and the empty list
  then says `1 geladene folge ist archiviert und ausgeblendet` with
  `archivierte einblenden` under it — then the podcasts followed. while the
  search field holds something it searches for podcasts rather than the
  library — apple's podcast directory (the itunes search api: free, no key),
  asked once the field stands still — and the column lists what it found. as
  the radio's stations do, a row offers a word at its end under the pointer:
  `folgen` for one found, `entfernen` for one followed (`gefolgt` stands
  there while the pointer is elsewhere); a click on the word follows or lets
  go, anywhere else on the row chooses the podcast.
- **the episodes** — the second (video episodes are left out of an episode
  with a video and a sound file the sound is taken, one with only a video is
  not listed): those of the podcast chosen, the newest first, or under
  `alle` those of all followed, the newest first, each with whose it is.
  eighty at first, and eighty more each time the list's end comes into view,
  as `zuletzt` does in the library. under `geladen` the episodes fetched
  whose files are still there, the newest first, whether their podcast is
  still followed or not (kessler notes each one fetched in `podcasts.toml`).
  each is set as a record's head is in the track list: its podcast's picture
  small, the title in the bold face, and under it whose it is (under
  `alle`), the day, and how long it plays or where you are — `noch 12:30`,
  `gehört` (heard ones in the quieter tone). where the list comes newest
  first, `heute`, `diese woche` (the last seven days) and `älter` stand over
  its parts. under the pointer a row offers `archivieren` at the end of its
  title (`zurückholen` for one archived). one fetched says `geladen` in the
  accent (a line of its own under the facts, which are set in the small
  face; three lines: the title, whose it is and the day, then `geladen` and
  how far it was heard — `noch 18:29`, `noch nicht gehört` or `fertig
  gehört`; the row's picture is always the podcast's, the episode's own
  stands in the third column). the one playing has the arrow before its
  title, an archived one's picture is quieter, and the podcast chosen in the
  first column — or `alle`, or `geladen` — wears the arrow and the darker
  plate, as a chosen view does everywhere. a right click opens its menu:
  `abspielen`, `in die warteschlange`, `archivieren`, `ab hier archivieren`
  — this one and every older one under it, to clear a long `alle` down — and
  `datei löschen`; on one of several picked, the same for all of them.
  episodes are carried into the queue as tracks are: picked up from the list
  — the ones picked together, where it is one of them — and let go at a
  place in the queue, a hair in the ink saying where; a row of the queue is
  carried to another place in it the same way. carried while the queue is
  put away, it comes out for them, and stays out once they are put down in
  it (as the music's queue does); archived episodes go into it too, by the
  row's word, the menu or a drag. `w` puts the queue column away and brings
  it back (`[ui] podcast_queue`); with `informationen` or the log in its
  place, `w` brings the queue back. empty, the queue says how something gets
  in, in one line, as the music's does. an episode taken over by music or
  the radio goes back to the head of the queue, where it stood. the heads of
  the columns do nothing under the pointer where they have nothing to do,
  only their words at the end do; the queue's says how many are in it, and
  `leeren` under the pointer. podcasts that cannot be reached say so at the
  foot, where last.fm speaks, for a few seconds once the round of feeds is
  read — one by its name, several as how many (`3 podcasts sind gerade nicht
  zu erreichen`); the log has each with what went wrong (the message stood
  over the list, with `ausblenden`, and the list jumped as it came and
  went). so do an episode that will not fetch and a search that fails. `t`
  shows the log in the queue's place, and takes it away again. `i` shows
  `informationen` in the queue's place, as on the radio and in the music:
  the page of the podcast playing — picture, makers, how often it comes, its
  site, what it is about — with the episode playing over it, or, while none
  plays, the chosen podcast's; `i` again puts it away, and it and the log
  take each other's place. the darker plate on `alle`, `geladen`, `gemerkt`
  or a podcast goes over to an episode as soon as one is clicked, as it does
  between the music's columns; the view keeps its arrow. `geladen` with
  nothing fetched says so. the episode's column offers `teilen`: its title
  and podcast and an address anyone can open go on the clipboard — the
  episode's own page where its feed names one (`link` in the item), else its
  sound file. `datei löschen` (in the column and in the right-click menu)
  works on the episode playing too: it stops, where it was is kept, and
  played again it is fetched anew and goes on from there; one still coming
  in has its fetch let go of. this, the search field's words, a key — stayed
  inside kessler, and nothing pasted came in from outside. episodes are
  picked together as tracks are: with the command key held one more, with
  shift all from the one looked at to the one clicked; with two or more
  picked the third column offers to do to them all at once — `in die
  warteschlange`, `archivieren` (or `nicht mehr archivieren`, were they all
  archived), `dateien löschen`, `auswahl aufheben`.

the first column's head turns its order, as the artists' does (`[ui]
podcast_order`): `zuletzt` — whose newest episode came out last first, the
order it begins in —, then a–z (↓), then z–a (↑). a podcast with new
episodes says how many after its name: `2 neu`.

**new episodes**. while the podcasts are on, kessler reads every feed
followed again quietly — at the start, every half hour, and as the view
opens when the last look is more than five minutes back. how often is `nach
neuen folgen sehen` in the podcasts' tab (`[ui] podcast_check`): every 15 or
30 minutes, every hour, every three hours, or `nur von hand` — then only
`jetzt nachsehen` under it, or `aktualisieren` at the end of the first
column's head under the pointer, looks — at rest that place says how many
podcasts are followed, or how many a search found, as the stations' head
does — (the feeds are still read once as the view first opens, to have
something to show). an episode out since the podcasts were last left, and
not yet begun, heard or archived, is new: a dot in the accent before its
title, how many beside each podcast in the first column and as `{n} neu`
beside `alle` (only the podcasts' own view says it). leaving the podcasts
counts as having looked (`seen_at` in `podcasts.toml`); the very first time,
nothing is new. the head offers `folgen` / `nicht mehr folgen` for the
podcast chosen. a click picks an episode, a double click plays it; until it
plays, `lädt` stands where the row says how far it was heard, its dots
going.
- **the episode** — the third: the podcast's picture (fetched once its feed
  is read, screened as a station's is) — or, the episode's own where the
  feed gives it one (`itunes:image` in the item), fetched as it is looked at
  or played and then standing in its row in the list too, its title, its
  podcast, the day it came out, how long it plays, `geladen` once fetched;
  `abspielen` (or `weiter bei 12:30`); `datei löschen` for one fetched, to
  have the room back (not while it plays); `in die warteschlange` / `aus der
  warteschlange`; `archivieren` / `nicht mehr archivieren` (an episode
  archived is greyed in the lists, or left out of them — see below); its
  **chapters**, where it has any — numbered as a record's tracks are, the
  same numbers the timeline shows, lit while the hand is on that chapter
  there; listed in the feed (podlove's `psc:chapter`), in a file the feed
  points at (podcasting 2.0's `podcast:chapters`, json), or, in the fetched
  mp3's own id3 tag (`CHAP` frames) — the one playing marked, a click going
  there (and starting the episode there if it is not the one playing) — and,
  on the timeline too, while it plays: a mark where each begins — heard and
  not heard as with music, no other tint. with the pointer on the timeline,
  the chapter under it lights up and its number shows by the rule, over
  where it begins — in the compact header on the plate the volume's figure
  wears, in the super compact one small in the scheme's sharpest tone, and
  its row in the list wears the hover plate; the other way round, pointing
  at a row in the list lights its stretch of the timeline; and what it is
  about — and, the pictures its notes show, under them, each as wide as the
  column and as tall as it is (screened as the covers are; the one-point
  counting pixels left out). the links in what it is about — its own `<a>`s
  by their words, and any address written out — stand under it, a click
  opening each in the browser; a podcast's description has its links the
  same way. with none picked it says how one is.

  the podcast chosen has a head over its episodes, as a record's stands over
  its tracks: its picture, and beside it its name, who makes it (the feed's
  `podcast:person`s, else its author), how often it comes out — read off the
  days of its last episodes: daily, several times a week, weekly, every
  other week or monthly, on which day mostly, and about when the next is due
  — and its site as a link, and where the feed names one (`podcast:funding`)
  `unterstützen · steady.page`, a click opening it. a series (`itunes:type
  serial`) is listed from its first episode, by season and number where the
  feed counts them, and kept whole; an episode's place shows with its date,
  `staffel 2, folge 4` or `folge 4`; under them the start of what it is
  about, all of it after `mehr`. `alle`, `offen`, `geladen` and `gemerkt`
  have none, their episodes being of many podcasts. the same page, at full
  size, is what `i` shows of the podcast playing. the pictures are read two
  at a time, each made small as it is read, and let go of once they have not
  been seen for half a minute. they are kept small on the disk too: apple's
  pictures — most podcasts' — are asked for at 600 × 600 rather than the
  3000 × 3000 a feed names, and any other is made no bigger than 600 points
  as it is fetched, one at a time; one kept big before is made small the
  next time it is read; and fetched one after the other on one thread of
  their own, which hands the memory back after each (two at a time on a
  thread each which left each thread's unpacked picture behind — fifteen
  podcasts' came to half a gigabyte; all at once before). switched off, the
  podcasts' feeds go from memory too, every episode of every one, and are
  read afresh when they are switched on again. on linux kessler keeps to two
  allocation arenas, so what the threads reading feeds let go of comes back.
  only a podcast's newest thirty episodes are held, and any begun, so that
  `offen` keeps them; at the end of a podcast's list `ältere folgen laden
  (770)` reads its feed again and holds it whole while it is the one chosen.
  a feed comes whole over the network either way — rss knows no pages. feeds
  are read four at a time. what is held of each feed is kept on the disk too
  (`podcast-feeds/` beside the pictures), with what its server said of it —
  its `etag` and `last-modified` — so kessler starts with the podcasts there
  at once, and asks each server only whether anything changed since: one
  that has not answers `304` and sends nothing. feeds are asked for
  compressed. a feed that has moved for good — a permanent redirect (301,
  308) or its own `itunes:new-feed-url` — is followed at its new address
  from then on, said in the log. what the podcasts do out of sight stands at
  the foot, where last.fm says its: `podcasts sehe nach neuen folgen` while
  feeds are read (with how many, where several are), `podcasts lädt 45 %`
  while an episode is fetched; nothing while neither.
- **the queue** — the fourth, where the window has room for it: the
  podcasts' own, not the music's, laid out as the music's: what it has
  played, faint — an episode heard to its end, or only begun before another
  was put on, the last fifty, five of them in sight above the one playing
  and the rest further up — then the episode playing in the accent, then
  what comes after it in the second tone. a double click moves it as the
  music's queue moves its needle: on a played row, what came after it and
  the one playing come back as what is to come; on a row to come, the rows
  skipped become played. an episode stands in it once: the one playing
  cannot be put in again (`läuft gerade`), and a played one put in leaves
  the played rows. a played row goes with `entfernen`, and `leeren` empties
  both; once one is heard to its end, the next is fetched and
  played. `entfernen` at a row's end under the pointer, `leeren` at the
  head's; a click looks at one, a double click plays it now. one
  still to be fetched keeps its place meanwhile and says `lädt`, its dots
  going, the one playing staying on top until the new one starts; only
  then does it leave the queue, and one that cannot be fetched stays. kept in
  `podcasts.toml`.

a click on the episode playing in the header goes to the podcasts with it
looked at, as a click on a track goes to the library and one on a station to
the radio — from any view.

**kept, heard, asleep**. `merken` in the third column or the right-click
menu keeps an episode on purpose: it stands under `gemerkt` in the first
column (there while any are kept), says `gemerkt` in its row, and is never
archived or let go of by itself — not by `ab hier archivieren`, `archivierte
dateien löschen` or `geladene löschen nach`. `als gehört markieren` / `als
ungehört markieren` there too, for the ones picked together as well. while
an episode plays, its column has the sleep timer: `einschlafen: aus`, then a
click each for 15, 30 or 60 minutes or the episode's end; the last eight
seconds fade out, then it pauses (at the episode's end, nothing from the
queue follows). while it runs, the foot says how long is left, `einschlafen
in 12 min`, wherever the window is — in the music too; under the pointer the
words give way to `+5 min`, five minutes more (set for the episode's end:
what is left of it and five), and `beenden`, off — fading out already, the
sound comes back. a feed's address typed into the search field (`https://…`,
`feed://…`) finds that podcast itself, to follow — a private feed, or one
the directory does not list.

the second column's head offers `archivierte ausblenden` / `archivierte
einblenden` at its end under the pointer (`[ui] podcast_show_archived`).

while an episode plays, the header's arrows are how far they skip — `−10`
and `+10`, as far as the podcasts' tab says — and repeat and shuffle are not
there; the media keys skip the same way.

the columns share the row out evenly, as the library's and the radio's do,
and the gaps between them are handles: dragged, the column on the left takes
the width; a double click shares the row out evenly again (`[ui]
podcast_shares`; every start shares it out evenly, as the library's and the
radio's columns too — a drag holds until kessler is quit).
- **listening.** an episode is fetched whole into a folder of its own beside
  the library (`podcasts/` in kessler's data folder; the third column says
  how far), then played as a file beside the queue: the queue keeps its rows
  and its place, its repeat and shuffle leave the episode alone, and after
  it play goes on where the queue stood. taken up where it was left, so it
  can be skipped through like any track. one that starts from its start
  plays as soon as a megabyte of it has come in, and can be skipped through
  as far as has come; one taken up further in waits until all of it is
  there. heard to its end (within half a minute of it), its file goes again.
  an episode is no song: it is not scrobbled, counted as a play, or looked
  up for words or an artist.
- **kept.** the podcasts followed and where each episode was left (every
  fifteen seconds while it plays, and when another starts), in
  `podcasts.toml` beside the settings. at the foot of the first column, how
  much the episodes fetched take on the disk.

the menu has a tab of its own for them while they are on:

- `zurückspulen` and `vorspulen` (`[ui] podcast_back`, `podcast_ahead`): 10,
  15, 30 or 60 seconds back, 10, 15, 30, 45 or 60 ahead — 10 and 10 to begin
  with.
- `geladene löschen nach` (`[ui] podcast_keep`), the archiving: `dem hören`,
  `7 tagen`, `1 monat`, `3 monaten`, `6 monaten` or `1 jahr` (the last
  three; files fetched longer ago go as the podcasts open and every half
  hour, heard or not), or `nie`. never the one playing.
- `archivierte folgen`: `ausgegraut` or `ausgeblendet` in the lists, as the
  second column's head switches it.
- `archivierte dateien löschen`: archiving an episode lets go of its file
  too; switched on, the files of those archived already go at once. never
  the one playing. under these rows the tab says in a few words what is
  what: archiving takes an episode out of the lists and keeps its file
  unless this is on; `datei löschen` only frees the room and keeps the
  episode listed; `geladene löschen nach` clears files away by itself,
  archived or not.
- `alle geladenen löschen`, with what they take beside it.
- `bilder neu laden`: every podcast's picture let go of and fetched again
  (they are kept in `podcast-art/` in kessler's data folder).
- `podcasts importieren…` and `podcasts exportieren…`: the podcasts
  followed as an opml file, the list every podcast app reads and writes —
  written to the desktop as `kessler-podcasts-<date>.opml`; read in, every
  podcast in it followed that is not yet.

in a narrow window (under 640 wide) the columns slide as the library's and
the radio's do, one at a time: `podcasts`, `folgen`, `folge`, and
`warteschlange` or `log` where it is open, the neighbours named at the foot
with an arrow — a click there, or `h` and `l`, slides on. choosing a podcast
goes on to its episodes, choosing an episode to its page; `t` goes to the
log. the columns run down into the foot as the library's do, each for itself
one with words at the foot under it goes up out of their way, the others
stay down. the darker plate stands on what is chosen in the column clicked
last; in the others what is chosen keeps its arrow or its bold, as in the
music (a podcast or view lost it once an episode was chosen, and an episode
kept it wherever the click went).

not yet: fetching ahead.

### looking for a cover

`cover suchen…` (right-click a record) asks, side by side: **last.fm**, when
a key is set up — the picture its listeners settled on, first — then the
cover art archive through musicbrainz, deezer and the itunes search. last.fm's
grey placeholder star is left out; its picture is taken at the size it was
uploaded, or at the size shown where that will not come down.

a cover — like every tag the editor writes — goes into a copy of each file
first (`.kessler-<name>`, beside it). the copy is read back strictly, and only
when it still opens, still runs as long as before and, for a cover, still
names the same title, artist and album does it take the file's place.
otherwise the file is left as it was and the status line says why.

reading, each field is taken from whichever of a file's tags has it, the
primary one (id3v2 in an mp3) first — a title only in the id3v1 tag at the
end still counts. and a track that came out with neither artist nor album
is read again at every scan, even when its file has not changed, so a fix
reaches it without a full rescan.

### a measurement

**menu → über → `messen`** writes down how the window is doing, for when it
seems to work harder than it should. the row is only there while `alt` (`⌥`)
is held down in the menu, or while a measurement runs. while it runs, every
five seconds one line goes to `kessler-messung.log` beside the library: how
long the five seconds were, how many frames were drawn and how many a second
— in brackets, how many of those came with no pointer moving and no key
pressed — how long kessler's own part of a frame took (the middle one, the
one 95 % stay under, the longest), how long the longest logic part of a
frame took (playback, the database, what is told to the system) apart from
the drawing and in brackets which step of it that was, how much of one
processor core the program used, the memory it holds now and in brackets
the most it has held, what the
textures hold (`texturen 34 mb in 210 (schrift 16384×64, grösste cover-… 0.3
mb)` — every cover and picture and the glyphs' atlas, in megabytes, how
many, the atlas's size and the biggest of the rest), and what was going on —
`musik`, `vorne` / `hinten` / `unsichtbar` for the window, `skop` (the scope
in the search field), `menü`, `tastatur`, `cover-laden`, `podcasts` (their view open),
`feeds-lesen`, `folge-laden`. last, in square
brackets, the three places in the code that asked for most of the frames
nobody's hand asked for, as `file.rs:line ×count`. click the row again to
stop; the file is then shown in the finder. started with
`KESSLER_MESSUNG=<file>` in the environment, the window measures from its
first frame into that file.

the podcasts say what they do too, each in a line of its own with the time
and the memory held at that moment (`· 330 mb` at its end, as every line
between the five-second ones has it): `ansicht: …` whenever the window shows
another view or list (`musik`, `radio`, `podcasts · alle`, `podcasts ·
geladen`, `podcasts · podcast <name>`, with how many pages further it was
scrolled and how many feeds are held), `neue folgen suchen: 40 feeds` and
`… fertig in 12.3 s · 5 neu`, each feed as it is read (`feed gelesen:
host/…/end · 7.2 mb · gelesen in 340 ms`, then `· 412 folgen · 2.1 s
(gewartet 0.4 s)` — or `unverändert`, or `fehler`), a feed taken from the
disk (`feed von der platte: …`), a search (`podcast-suche: 25 gefunden ·
0.8 s`), and an episode asked for (`folge laden: podcast — title · 96 mb`)
and come in (`folge geladen: 96 mb in 14.2 s`). what was said before the
measurement began keeps the time and memory of when it was said, and a
feed's address is written without what follows `?` or `#`, where a private
feed carries its key.

between those lines, what the sound of a podcast's episode is, each with the
time: which episode (podcast, title, address, whether it plays as it is
fetched or whole), the file's channels, rate and length as it opens (`ton:
…`), every change of them part way through and how far in (`wechsel nach …
s`), every jump, its end, and the output device's rate and channels as it
opens (`ausgabe: …`). an episode that changes its sound part way — an
advertisement put in, made another way — is kept at what it began with,
rather than played too fast or too slow. the device's name stands in
`ausgabe: …` too; while something plays the device is asked every three
seconds whether it still runs at the rate it was opened at, and a change is
written (`ausgabe gewechselt: … jetzt … hz (geöffnet mit … hz)`) — a headset
switching to its microphone's mode, or another program setting the device,
would play the sound at the wrong pace. and what was said before „messen"
began is kept for it (the last 200 lines), so a measurement begun with the
episode already playing still has how the file and the device opened. a jump
within a file — a click on the timeline, the skip keys, a chapter — is gone
to by the thread that reads ahead: the jump comes back at once, and a moment
of silence is heard until the new place is there. a place the file cannot be
taken to is said in the measurement (`ton: sprung nach … ging nicht`), and
it plays on where it was.

### the log

much of what kessler does happens out of sight — a scrobble four minutes into
a track, years written into files by a run through musicbrainz, a transfer to
the ipod — and the status line says each of those once, for a few seconds. the
log is the rest of the time: every such thing with when it happened, newest
first, in the fourth column. `t` opens and closes it, and so does a click on
the status line (what it says now is one entry of the log); `w` swaps the
queue back.

what goes in: scrobbles and anything else last.fm says, tags edited by hand
(which tracks, which fields), years and genres from musicbrainz, covers put
on records, music sent to the ipod, ejecting and pulling it out, a scan that
found new or changed tracks — and, under its line each track it read again,
with what came out different („aktualisiert: artist – title: jahr 1999 →
2001, genre leer → jazz") or, where nothing did, why it was read at all: the
file is newer but says the same, its record's cover has not been looked for
yet, or it has neither artist nor album and is read every time (up to 200
such lines a scan) —, music added, a folder taken out, lists made, renamed,
deleted and added to — and, the podcasts: followed and let go of, episodes
fetched, files deleted by hand or after so many days, episodes archived, new
episodes come in (which podcast, how many), a feed or an episode that could
not be had, the podcasts saved or read in. in the podcasts' view, `t` shows
the log in the queue's place. the last 1000 entries are kept in `log.txt`
beside the library's database, one line each — so what was scrobbled
yesterday is still there today. with the pointer on the column's head, its
count turns into `leeren`, which empties the log and the file.

### lists

the lists live in the **fourth column**, taking turns with the queue: `u`
swaps them in, `w` swaps the queue back. their
names stand at the top — each with its length, and `neue liste…` at the foot
— and under them the list that is open, to be read, filled and rearranged.
above them, set apart and muted, stands `zuletzt hinzugefügt`: the one list
nobody made, the library's newest tracks — the same ones as the `zuletzt`
row, and the same ones the ipod keeps as a playlist. a click puts the first
column on `zuletzt`, a double-click plays it; it cannot be edited, and the
keyboard's walk through the lists passes it by.
a click on a name opens it there; the three columns to the left go on being
the whole library, so putting a list together is browsing with one hand and
dragging with the other. a double-click plays the list.

the queue itself stands on the ground like the other three columns, with no
field behind it. what has been played is faint, what plays is in the accent
(its artist in the receded accent, chosen or not, unless the keyboard's marker
is behind it), what is still to come — title and artist alike — is in the
second tone: three
colours down the queue. its head says how much is still to come — `noch 81` —
rather than how long the evening has been.

**the device, looked at.** it opens with `ipod` in the header (the device's
own name is in the column), in the accent while the columns read the
device, which also reads the device in the columns — clicked again, both go
back. the column says it all and ejects. the fourth column then shows the device instead of the queue, the log
or a list: a small player at its head, seen face on, set in the same dots and
the same accent as the covers. over it the pointer is a hand: dragged, the
player turns with it like a ball under the hand, every way, as quick look
turns a model on the mac — whichever way it stands, the side under the hand
goes where the hand goes, and going round in circles rolls it. let go, it
stays as it is for two seconds and then turns back face on, the short way
round, easing in and out. while a sync writes to it, it spins about the
screen's upright, a turn every four seconds, and turns back face on when it is
done. its screen is dark glass with a band of light mirrored in it that slides
across as the player turns. the column's head says `ipod`, then the device's
own name after a dot. under the picture what can be read off it — its name,
model and serial number, tracks, records, artists, playlists, how long it all
plays, the formats, when its database was last written, and how full it is, as
a bar in the timeline's language: the music in the ink, anything else on the
disk in grey. then what the library has that the device lacks, and what of
that this model could not play; `synchronisieren…` opens the same transfer a right-click does, and `auswerfen` is there
too.

**renaming it.** `name` stands first; clicked, it is a field in place, the
name marked, so the next key replaces it: enter writes it, escape leaves it.
the name is the one the ipod goes by itself — under settings → about, and the
one kessler shows for it everywhere; it is the name of its master playlist.
the database is backed up first, as for a sync. the volume keeps its own
label: that one you rename in the finder or the file manager. a device whose
master playlist has no name is shown by its volume's.

**a report.** `bericht speichern` — there only while ⌥ (alt) is held — writes `kessler-ipod-bericht-<date>.txt` to the desktop (the home
folder where there is none) and shows it in the file manager. it says what the
device will make of its database: whether the master playlist carries the five
indices the menus are built from, which artists and albums would stand more
than once without them, names that differ only in case, accents or spaces,
tracks without a file and files without a track, play counts waiting, backups,
and what the next sync of the whole library would do. a spelling
only tracks the library does not have use is marked `(nur auf dem ipod)`, the
menus say when the indices make the order of the master playlist beside the
point, and each track whose tags the next sync would change is listed with the
fields, as the ipod has them and as they will be. it names artists and albums
and nothing else of the library — made to be sent along when something on the
ipod looks wrong. `kessler ipod report` prints the same.

**the track list in the master playlist's order**. the menus are built from
indices that name each track by its place, and a place in the track list and
in the master playlist are the same on every ipod itunes writes. kessler wrote
the master playlist in the artists' order and the track list in the order
tracks came, and a 3g read the places the other way: artists twice and out of
order. now both lists go out in one order; a device written before is written
again by the next sync (`menüs · neu ordnen`), and the report says `titelliste
wie die hauptliste geordnet: ja` or `nein`.

**laid out as libgpod lays it out**. the master playlist carries, after its
name, the long column mhod (type 100) itunes and libgpod write, then the five
indices in libgpod's order — title, artist, album, genre, composer — each with
its jump table, whose letters are worked out as libgpod does. a device written
otherwise is written again by the next sync, and the report says `nicht in
libgpods reihenfolge`.

**sorted as itunes sorts**. the indices put names beginning with a letter
first, then those beginning with a digit, numbers compared as numbers, marks
before the first letter left aside (`(((microwaves)))` under m), and an empty
name last. a 3g whose artist index began with the one track without an artist
showed no artists at all.

**one spelling for one name.** the library takes `Qluster` and `qluster`, or
`Amon Düül` and `Amon Duul`, for one artist and shows her under the spelling
most of her tracks carry. the ipod compares names as written and lists each
spelling as an artist of its own, her records split between them. so every
artist, album artist, album and genre goes onto the device spelled the
way the library shows it (the most frequent spelling, ties alphabetically),
also when only a selection is sent; tracks already there in another spelling
are retagged by the next sync. the files keep their own spellings. the same
goes for tracks only the device has: their artist, album
artist, album and genre are spelled as the library spells the name, or tidied
where the library does not have it, and nothing else of theirs changes (the
device column says `schreibweisen · n angleichen`). an album or a genre on the
ipod of spaces alone is cleared where the library has none. a track on the
ipod that two of the library's find — one song in two releases, another year
and number — is left as it is while one of them agrees with it, rather than
given the other's tags at every sync.

**zuletzt hinzugefügt, on the device.** every `synchronisieren…` also keeps
a playlist of that name on the ipod: the library's newest tracks, as many as
the `zuletzt` row shows at first (80), newest first. it is written whole in
place of the one before — same playlist, new contents — and only when it has
changed; the column says `aktuell` or `nicht aktuell`, and
`synchronisieren…` is offered for a list that is out of date even when no
track is missing. it is known by its english name too (`recently added`), so
switching the window's language does not leave two of it behind. your own
lists are not sent; the device's own playlists are kept as they are.
`schliessen` in the column's head gives it back to the queue. the figures
are worked out once when the column opens, and again only when the device
changes.

**putting one in order**: the open list's head carries its name and length,
and under the pointer the word `schliessen`, which closes it (the names
stay). `schliessen` on the `listen` head hands the whole column back to the
queue. the queue is not disturbed by any of this and not emptied — it is
still there, still playing, just not what that column is showing.

what you can do in it:

- **drag a row** to another place in the list — a hairline in the ink shows
  where it would land,
- **drag tracks, a record or a name in** from the columns beside it, at the
  place you drop them. a picked row or record carries everything picked with
  it — after `⌘A`, the cover of any record carries them all,
- **take a row out** with `Backspace`, or its right-click → `aus der liste`;
  the file is not touched,
- **double-click** to play the list from that row on.

every one of those is written to the list the moment it happens. there is no
saving step and no `sichern` — a running order with a save on it is a running
order somebody loses.

the queue is still the other way in, and still worth it when what you want to
keep is an evening that already happened: right-click a list →
`in die warteschlange`, rearrange, then right-click the queue →
`als liste sichern…`.

**getting to them**: `u` brings them in (and opens the list that was open
last time); `Shift+u` opens the first list, and each press after that the next
one and round again. from the track list, `l` or `→` steps into the names, and
`j`/`↓` walks on down into the open list. `Enter` on a name plays that list,
`Backspace` asks whether it should be deleted.

**making one**: click `neue liste…` or press `Enter` on it, and the row turns
into the line you type the name in — `Enter` makes the list, `Esc` or a click
anywhere else lets it go. the new list opens under the names straight away,
empty and ready to be dragged into. tracks dropped on `neue liste…` start a
new list made of them. renaming works the same way, in the list's own row.

the other way, when you already have the music in hand: right-click tracks or
a record → `in eine liste…`, which asks for a name — typed in the lists, as
above — and suggests the record's or the artist's. adding to one without
opening it: drag tracks, a record or a name onto its row — the row lights up
and says which list it is. renaming and deleting are on the list's own
right-click; deleting asks first, and says what it does not take with it (the
music).

lists live in the library's database, next to the tracks. they are not written
to disk as `.m3u` files yet, and the ipod's own playlists are read but not
shown.

### adding music

a folder is taken out again by clicking its row — the path, under the pointer,
offers `entfernen` at the far right — and then answering the `ordner
entfernen?` that opens under it. taking a folder out takes its tracks out of
the library with it, which is further than one stray click on a row should go.

drop files or folders on the window, or use menu → bibliothek → `musik
hinzufügen…`. everything lands in the first configured music folder as
`artist/album/nn title.ext`, filed by its tags rather than by the folder it
came from, and is read in exactly as a scan would read it. copying is the
default; `dabei: verschieben` moves instead. nothing is ever overwritten.

on macos a third line reads `und an music.app: ja | nein`, off by default.
switched on, every batch kessler files is also handed to music.app —
`osascript`, `tell application "Music" to add {…}`, one script per hundred
files, which is the sanctioned way in: music's own database is undocumented,
and writing to it is how a library gets broken. music only knows what has been
added to its library, so a file lying in `~/Music` is not in it; this is what
puts it there.

two things to know before switching it on. with music's own "copy files to the
music media folder when adding to library" **on**, music makes its own copy and
the same music is on the disk twice — switch that off in music's settings and
both programs point at the one file. and the hand-over happens when kessler
files something; music already in the library from before is not sent
retroactively.

### the right-click menu

it lies over the rows on the search field's plate, with a faint shadow in the
scheme's own colour — its ground taken down towards black, wide and soft and falling a little
below it, as the menus of the system's own apps have it; the row under the
pointer takes the darker marked tone.

under a track's entries — in the track list, the queue and an open list —
its last row is five stars: as many lit as the rating, or, under the
pointer, as far as the pointer is; a click rates to that star, a click on
the star the rating already ends at takes it away. it rates the rows picked,
all of them where several are, and never more than those; the stars stand
lit where they share a rating.

### where a file is

right-click a track, a record, a row of the queue or a row of an open list →
`im finder zeigen`: the file manager opens on the folder with the file
selected in it. a record shows its first track, which is its folder; several
selected tracks show the first of them. on macos that is `open -R`. on linux
the entry reads `im dateimanager zeigen`, and asks whichever file manager is
installed through the one interface they all answer —
`org.freedesktop.FileManager1` over d-bus, which nautilus, dolphin, nemo,
caja and thunar speak — and where none does, opens the folder with
`xdg-open`, without the file picked out. on windows it reads `im explorer
zeigen` and is `explorer /select,`. a file that has gone since the last scan
is said to be gone rather than opening an empty window.

### the typeface

the window is set in the faces of the `fonts/` folder, one family to a
subfolder: regular, bold, and a mono cut for the figures. **menü →
darstellung → schrift** steps through them (`[ui] typeface`, the family's
key), and with ⇧ held the other way. the rest of the machine is offered only
when asked for, below. unset, it is **muto** where the build has it, else
**asap**; where neither is there, the first family in the folder. asap
chosen in the menu is kept by its name so it stays.

**asap** (omnibus type, version 3.002) comes with the repository, free under
the sil open font license, unchanged, as one variable file in `fonts/Asap/`.
every weight of it sets every letter in the same width, so a name turning
bold does not grow and nothing beside it moves. with a face like that, a name
in a list thickens over a tenth of a second when its row is chosen, and thins
again when it is left, rather than switching cut in one frame: the window
asks the face for the weights in between (regular 400, bold 550, in twelve steps). a face without a weight axis switches as before. its figures are of
one width as they are.

asap also has a width axis, from 75 to 125: **menü → darstellung → schrift →
breite** (`[ui] font_width`, 100 is the face as drawn) sets the whole window
narrower or wider. a first start opens at 85 (at 75, the narrowest). the
slider is there only for a face with that axis, and the width is taken when
the hand lets go.

a face with a weight axis — asap — has a second slider beside it,
**gewicht** (`[ui] font_weight`, 400 is the face's regular). it sets how
heavy the regular is drawn; the bold goes along two hundred above, so a
chosen name stands out by the same step at every weight, and the japanese
behind the face follows both. on a light ground both are set 80 heavier than
the slider says: light words on a dark ground spread a little into it and
dark ones on a light ground thin, so the same weight reads lighter there.
the faces are as wide at every weight, so nothing moves when the scheme
turns from dark to light.

**alternativen** under schrift lists every letter the face in use can draw
another way, each on its own row: `klein a`, `3` (the capitals' alternates
not, the window sets everything small; nor, what has no letter or figure
under it — æ ŋ, © ® ℗, №, the arrows) — with the feature it comes from
beside it (`ss01` …, `zero`); a click turns it on or off, bold is on, and
the faces are installed again at once. two rows for the same letter or
figure — muto's 0 with a slash (`zero`) and with a dot (`ss10`) — are one or
the other: turning one on turns the other off. a feature that redraws
several letters (muto's `ss04`: a, m, n, v, w and v, w) is a row for each,
so each can be chosen alone; accented forms go with their letter. the choice
is kept for each face apart, in `[ui] font_sets` (`asap = ["ss01:g"]`, `muto
= ["ss01:a"]` from the first start: asap's single-storey g, as it always
drew and muto's open a, its two-storey g). folded, the row says what is on.
egui sets text with a face's default features — its ligatures and kerning —
but cannot be asked for a stylistic set, so kessler writes the face's
character map anew when it loads it, pointing each chosen character at the
drawing the face keeps behind the feature. an alternate of an alternate is
offered as one of the plain letter: muto's dotted zero is `ss10` on the
slashed zero that `zero` makes, and stands as `0 · ss10`; with both on, the
dotted one is drawn. the ligatures are no row: egui joins them itself,
always — ff, fi, fl, ffi, ffl, and in muto -> and <- as arrows and (c), (r),
(p) as ©, ®, ℗. in muto the labels — the small tracked heads and words about
the interface — are always set with the open a (`ss01`), whatever
`alternativen` says for the rest: kessler gives the face a character of its
own for it and sets a label's a with that.

changing the face takes what goes with it at once — its tracking, the medium
of the small sizes, its square ends, the labels' open a.

`hinting` under schrift (`[ui] font_hinting`) says how the letters are
fitted to the screen's pixels: `voll` (the default, egui's own: stems
snapped to whole pixels), `leicht` (only heights snap, the shapes across are
left as drawn) or `aus` (the outline as the face draws it, as a pdf shows it
— softer at small sizes, but no letter bent out of shape: in muto at label
size the h could come out oddly).

**junicode** (peter s. a config still naming it gets asap. a stylistic set
that redraws most of the alphabet is still one row under `alternativen`
(`ss06:*`), not a row a letter.

in muto the labels — the heads of the columns and of the menu's sections,
the controls along the top — are tracked a touch less than in asap, 75 % of
it: its letters already stand further apart. and a record heading's line
under the name stands closer under it in muto, 0.84 of a row rather than a
whole one: muto's small letters are short beside its capitals and figures,
and the full row looked like a gap. in muto everything at the small size is
set in muto medium — the controls along the top, the small heads, the log,
the small figures (in muto mono medium), the playing title among them too,
which elsewhere is bold — and everything at the large size in regular and
bold: medium for small, regular and bold for large, nothing else (labels
every small size). `fonts/Muto/` holds the medium cut again for it (and muto
mono's, for muto mono); without the file the labels stay regular.

**muto mono** is the monospace on offer after muto under schrift: out of
`fonts/Muto/` it is muto's mono cut, lent to a family of its own. it is
gone, and a config naming it opens in asap. it has no arrows; those come
from asap, which stands behind every other face for the marks it lacks.

**capitals**: with ⌥ held, under darstellung, a row **grossbuchstaben**
turns them on (`[ui] capitals = true`, off). the names from the files, the
stations and the podcasts then stand as they were written — `Boards of
Canada`, `NTS 1` — instead of all in small letters; the window's own words
are written small in the first place and stay so. while it is on the row
stays.

**the machine's own faces**, for trying out: with ⌥ held in the menu, a row
**systemschriften** shows under schrift. turned on (`[ui] system_fonts =
true`), schrift steps on past the folder's families into the ones installed
on this machine — on a mac `~/Library/Fonts`, `/Library/Fonts` and
`/System/Library/Fonts`; on linux `~/.local/share/fonts`, `~/.fonts`,
`/usr/local/share/fonts` and `/usr/share/fonts`. each file is asked for its
family name, weight and slant: the upright face nearest 400 is the regular,
the one nearest 700 the bold. collections (`.ttc`) and faces without the
latin letters (symbols, emoji) are passed over. while it is on the row
stays; turned off with one of them set, the window goes back to asap.
nothing of these faces is copied anywhere.

**muto and muto mono**, where they are installed on the machine (in the same
folders), are offered under schrift without ⌥, after asap: muto sets its
figures in muto mono, and both are set a size down, fourteen for fifteen.
only files with „muto" in their name are looked at, so this costs nothing at
start. read where they lie, nothing of them is copied.

**muto** comes with kessler again in `fonts/Muto/` (regular and bold of muto
and of muto mono), where the build has it: rade matic licenses it to travel
with kessler — in the program, the.app and the packages — and to be used
there, and nowhere else: not taken out and installed or used elsewhere, not
passed on alone or in other programs, not changed or sold. muto is not under
kessler's gpl-3.0; its terms are in `fonts/Muto/LICENSE-muto.md`, and listed
under über → lizenzen. a copy built without the folder simply has no muto to
offer.

asap took its place.

the transport's shuffle and repeat are built by the window out of a face's
arrow, and repeat-one is a ring with a „1" in it, drawn in the same stroke.
for a face with no arrow at all the window builds all five out of its
hyphen.

a face with an arrow but no shuffle or repeat has those two built out of its
own arrow — its stem, its head and the thickness of the head's arms measured
off the drawing and set again: two lanes that cross for the shuffle, the
lower one broken where it passes under the other, and two lanes turned into a
loop for the repeat (`src/gui/glyphs.rs`, `compose`).

a folder holding none of them offers whatever it does hold. to try another
face, use the machine's own faces below ⌥.

```
fonts/M PLUS 1 Code/MPLUS1Code-Variable.ttf     (in the repository, for japanese)
fonts/Asap/Asap-Variable.ttf                    (in the repository)
fonts/Muto/MutoRegular.otf … MutoMonoBold.otf  (only within kessler; not in the public repository)
```

every build — `make-app.sh` with or without `--share`, and the linux packages
— carries only these.

`bundle/make-app.sh` copies that folder into `kessler.app/Contents/Resources`,
so the finished application carries its own typeface and can be moved anywhere.
`[ui] font_dir` points somewhere else if the folder lives somewhere else. with
no folder at all the window falls back to egui's built-in face, which looks
wrong on purpose — a bold cut is the only selection signal in this design, and
without one the interface goes quietly flat. the menu says so when it happens.

a face the chosen one has no glyph for — a japanese title, say — is drawn in
m plus 1 code (m plus rounded 1c before), which comes with kessler (sil ofl,
like asap) as one variable file. a bold title stays bold through its
japanese, at the weight the window's bold is drawn at, and every weight of
it is one width — so a japanese name thickens with the rest of its row, as
asap's do, and nothing moves. behind muto and muto mono, which have no
weight axis, its regular is set at 520 rather than 400, where its strokes
are as heavy as theirs. it is set a little smaller than the chosen face, so
that a kanji stands just over that face's capitals — a kanji fills its
square, a capital does not — and a japanese title is no taller than a latin
one beside it. it is not offered as a typeface of its own. what it lacks too
— korean, the rarer chinese — falls to whatever the machine has; `menü →
darstellung → ersatzzeichen` names the face, and `[ui] font_fallback` points
at a different one in place of both.

the fallback faces are mapped from their files rather than read into memory:
only the parts the window's glyphs come from are loaded, and the system can
drop them again. on a mac the machine's face is usually arial unicode, some
23 mb, so this keeps about 30 mb out of kessler's own memory.

## an application you can double-click

```
./bundle/make-app.sh                 # builds ./kessler.app
./bundle/make-app.sh /Applications   # or straight into applications
./bundle/make-app.sh --share         # dist/kessler-<version>-macos.zip and .dmg, to pass on
```

the `.dmg` opens on a drawing by felix h-w: a cream ground, the arrow from
kessler to applications and two dogs on a seesaw — in 公園 hell's colours the
ground, the drawing in its receded accent and the words in its three tones
(a disk image has one picture for light and dark alike); cream and blue
before — and in the top left corner `kessler`, the version and the commit as
the menu's head says them. kessler.app stands at the top right, the link to
applications at the left under the arrow, 80-point icons, no toolbar. the
window opens 680 × 700 points, the picture's size; the cream goes on past it
(to 2000 × 1300), so a larger window — or a tab, where the finder is set to
open them so (system settings → desktop & dock → prefer tabs), and a disk
image cannot say otherwise — shows no white beside it. the picture is
`bundle/dmg/background.png` and `background@2x.png`, made from the scans in
`bundle/dmg/` by `bundle/make-dmg-background.py`; `make-app.sh` writes the
version into it with `bundle/dmg-stamp.swift`, joins the two sizes with
`tiffutil` and has the finder set the window up (the first time macos asks
whether the terminal may steer the finder). where swift, `tiffutil` or the
finder are not to be had, the plain image as before. the steps live in
`bundle/dmg-dress.sh`, which `notarize.sh` uses too. an image of the same
version still open from the last build is ejected first.

github builds the same and hangs the `.dmg` and the `.zip` on the release of
the version: by itself when a merge into main raises the larger part of the
version, and by hand whenever it is wanted — actions → `mac package` → run
workflow. whether the version rose is asked on a linux runner first.

**to give it to somebody else, use `--share`**: it carries the same faces
as the app built for your own mac, checks the bundle for anything else before
it packs it, builds for apple silicon and intel in one binary, signs it ad hoc
and zips it with `ditto`. the other mac has to be told once that the app is
fine, since no developer signed it: unzip, drag `kessler.app` into
applications, then right-click → open, or system settings → privacy &
security → open anyway. `xattr -dr com.apple.quarantine /Applications/kessler.app`
in a terminal does the same.

**signed and notarized**: `./bundle/notarize.sh` after `--share` (or
`./bundle/notarize.sh --build` for both) signs the app with a developer id,
sends it to apple's notary service, staples the ticket to it and packs the
zip and the dmg again, the dmg signed and notarized as well — a mac that
downloads them opens kessler without being told it is fine. it needs an
apple developer account and two things set up once on the mac that signs,
both described at the head of the script: the „developer id application"
certificate in the keychain (`KESSLER_SIGN_IDENTITY`) and a notary profile
(`xcrun notarytool store-credentials`, `KESSLER_NOTARY_PROFILE`,
`kessler-notary` by default); nothing secret is in the repository. it runs
under the hardened runtime with one entitlement, apple events
(`bundle/kessler.entitlements`), which the import into music needs; it
refuses a bundle with a licensed face that may not be passed on; muto goes
along under its own licence. for this the app is called `de.windparksoft.kessler`:
after the first build under the new name macos asks once more for the
permissions it had (controlling music); the settings and the library do not
hang on the name.

the icon is a k on a tile. the one that ships — and that the system shows
before kessler runs — is muto bold's k, white on black
(`bundle/make-icon.py` falls back to asap where muto is not there, as in the
public repository). once the window is up it draws the icon again itself, in
what the window is dressed in: the k in the typeface it is set in — for
asap, or any face with those axes, at the same 700 and 85 whatever the
window's own width and weight, so the k does not change as the app opens and
closes; for another face its bold — in the scheme's accent, on the scheme's
ground, as tall as the shipped one shows (on a mac 0.56 of the tile, as
macos 26 draws the layered icon, and elsewhere 0.56 of the canvas) and
centred halfway between its outline and its weight, so its stem does not
pull it to the left, and again whenever the scheme or the typeface changes
(`src/gui/appicon.rs`). on a mac that is the dock icon, handed to the
application directly; on linux and windows it is the window's icon. the
licensed faces never go into the shipped icon — only the window draws them,
on the machine they were bought for.

the tile takes the desktop's shape: the mac's (below) everywhere but on gnome,
where it is adwaita's rounded square standing on a lip a shade darker than its
face, and on kde, where it is breeze's square, nearly the whole grid, the
corners only just turned. the window asks `XDG_CURRENT_DESKTOP`, and
`install-linux.sh` does the same to put the right one in the icon theme.
`bundle/make-icon.py` draws the shipped ones (pillow, cairosvg, fonttools);
run it only when the icon changes, since its output is committed. a test
looks at what shipped: a white k on black, in every shape.

it ships as two icons, and each system takes the one it understands:

- **`bundle/icon.iconset`** — the flat one, for everything before macos 26.
  its shape is not a rounded rectangle but one with quarter-superellipse
  corners, which leave the straight sides with no jump in curvature — that
  break is what makes a foreign icon look foreign in the dock — on apple's
  grid of 824 units in a 1024 canvas. dressed as the dock's other icons are:
  a fine rim round the tile's edge, light at the top and darkening to the
  foot, and apple's soft shadow under it (12 units down, 14 soft, half
  black); the k stays flat. the icon drawn while kessler runs, in the
  scheme's colours, is dressed the same way, its rim dark instead on a light
  ground, where a light one would not show. `logo.svg` stays flat.
- **`bundle/kessler.icon`** — the layered one, for macos 26 and after, in
  apple's icon composer format: the black ground and the k, one look for
  light and dark; the system makes the clear and tinted looks out of the same
  layers. no glass on the k: glass would round and shade it. `make-app.sh` hands it to `actool`, which needs xcode 26 or later —
  with only the command line tools installed that step is skipped and 26 falls
  back to the flat icon. it opens in icon composer to look at or adjust;
  re-export it over either `bundle/*.icon` or `*.icon` in the project root and
  the script finds it and names the plist key after the file.

started outside a bundle (`cargo run`), the window hands the dock the flat
icon at the start, and then its own, as from the bundle.

a `.app` is only a folder with a fixed shape — the binary under
`Contents/MacOS`, a plist saying what it is called, an icon — so the script
compiles nothing that `cargo build --release` did not already compile. it only
assembles it.

launched from the bundle, kessler leaves the dock icon to the bundle's own
`.icns` until it has drawn its own. it knows which case it is in
from its own path: nothing but a bundle puts an executable in
`….app/Contents/MacOS/`.

the result is unsigned, which is fine for something you built yourself — macos
only demands a signature from software that arrived over the network, and a
local build carries no such mark. copying the `.app` to another mac would get
it stopped; that needs an apple developer certificate.

### closing the window on a mac

the red button puts the window away and the music plays on, the way
music.app does it: the whole program is hidden, as ⌘h would, and a click on
its dock icon brings the window back as it was left. ⌘q, `q`, or „beenden" in
the dock's menu end it. elsewhere the button ends the program, which is what
it means there.

## on linux, and on a raspberry pi

there is no bundle to build on linux: an application is a line in the start
menu, an icon in the icon theme, and the binary they point at.

```
./bundle/install-linux.sh              # build, then install
./bundle/install-linux.sh --no-build   # install what is already built
```

writes `~/.local/share/applications/kessler.desktop`, the icon at every size
it was drawn at into `~/.local/share/icons/hicolor/`, and a link
`~/.local/bin/kessler`. nothing needs sudo. the entry points at
`target/release/kessler` in the project rather than at a copy, so the
typefaces are found beside it and the next `cargo build --release` is what
the menu starts. the window calls itself `kessler` (the wayland app id, the
x11 class), which is how the task bar puts the icon beside it.

**a raspberry pi, ready built** (a pi 4 or a cm4, raspberry pi os 64-bit
with the desktop — `uname -m` says `aarch64`; bullseye or later). github
builds kessler for the pi and hangs it on the release of that version, so
nothing has to be compiled on the pi:

```
gh auth login                                  # once
gh release download --repo radiorauschen/kessler --pattern '*linux-arm64*' --clobber
tar xzf kessler-*-linux-arm64.tar.gz           # makes kessler-linux-arm64/
cd kessler-linux-arm64 && ./install.sh         # the start menu entry and the icon
```

an update is the same first three lines in the same place: the folder has no
version in its name, so the new package lands where the menu entry points.
`install.sh` only has to run again if the folder moves.

**an ordinary linux pc** (x86_64, any distribution from about 2020 on — glibc
2.31) gets the same, as `kessler-<version>-linux-x86_64.tar.gz` next to the
pi's on every release: `--pattern '*linux-x86_64*'`, then `./install.sh` in
`kessler-linux-x86_64/`. without access to the repository, the file itself
can be passed on — unpacked anywhere, it works.

`bundle/make-linux-package.sh` is what builds them — anywhere with rustup,
python and curl, a mac included: it fetches zig, `cargo-zigbuild` and, where
there is none, pkg-config (pypi's `pkgconf`, ready built — homebrew builds
its own from source and fails on a mac whose compiler is out of date)
into python's own folder for the user, links against glibc 2.31 (so
the result starts on raspberry pi os bullseye, ubuntu 20.04 and everything
after them) and against an old alsa, and packs the program, the free
faces with their licences, the icon and `install.sh` into
`dist/kessler-<version>-linux-<arch>.tar.gz` — `arm64` by default, `x86_64`
when asked (`./bundle/make-linux-package.sh x86_64`).

`bundle/install-linux.sh` in a clone builds instead, and says so first when
rust or the alsa headers are missing rather than stopping halfway.

**or built on the pi itself:**

```
sudo apt update
sudo apt install -y git build-essential pkg-config libasound2-dev libxkbcommon-x11-0 ffmpeg
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env
git clone https://github.com/radiorauschen/kessler ~/kessler
cd ~/kessler && ./bundle/install-linux.sh
```

the release build optimises the whole program at once, which on a pi takes
the better part of an hour and, at the end, a long quiet stretch while it
links — `top` in a second terminal shows `rustc` or `ld` still busy. if it
stops with `killed`, give it more swap (`CONF_SWAPSIZE=2048` in
`/etc/dphys-swapfile`, then `sudo systemctl restart dphys-swapfile`) and run
it again. afterwards, `git pull && cargo build --release` updates it.

ffmpeg is there for the ipod: on linux it does every conversion that
`afconvert` does on a mac.

the media keys work on linux too: kessler registers on the session bus as an
mpris player (`org.mpris.MediaPlayer2.kessler`), which is where kde, gnome and
most other desktops send play, pause, next and previous. the same registration
puts what is playing, with its cover, into the panel's media controls. seeking
from the panel is not offered. on a machine without a session bus (a pi
started from the console) there are no media keys, and nothing else changes.

### a tiling window manager, and other desktops

sway, i3, hyprland and the like:

- **window rules** match `kessler`, the window's wayland app id and x11
  class — in sway `for_window [app_id="kessler"] move to workspace 9`, in
  i3 `for_window [class="kessler"] …`. a narrow tile is fine: below 640
  points the window goes over to its narrow layout, whose columns slide, and
  it works down to 360.
- **keys.** the window's own keys work as anywhere. for keys that reach it
  from another workspace, bind the command line, which talks to the running
  window over the same session bus as the media keys:
  ```
  bindsym XF86AudioPlay exec kessler toggle
  bindsym XF86AudioNext exec kessler next
  bindsym XF86AudioPrev exec kessler prev
  ```
  `kessler play`, `pause` and `stop` are there too; `playerctl` does the
  same, if it is installed. `kessler now` prints what plays —
  `autechre — foil`, `(angehalten)` after it while paused, nothing while
  stopped — for a status bar: in waybar a `custom` module with
  `"exec": "kessler now", "interval": 5`.
- **the symbol in the panel** is a statusnotifieritem, as kde and waybar's
  `tray` show it; i3bar shows only the older xembed kind, and does not show
  it.
- **a notification for each new track** — menu → wiedergabe →
  `benachrichtigung` (`[ui] notify`, off at first): title, artist and record
  and the cover, by way of whatever shows notifications (mako, dunst, the
  desktop's own), while the window is not in front — it is often on another
  workspace. each takes the place of the one before rather than stacking.
- **the ipod is mounted by kessler** where nothing else does: without a
  file manager that mounts what is plugged in, an ipod stayed a disk nobody
  opened, and kessler never saw it. now kessler finds one by its name under
  `/dev/disk/by-id` (`usb-Apple_iPod…`) and asks udisks to mount it
  (`udisksctl mount`, as the person sitting there, no password); the status
  line and the log say where it went, or why not. menu → ipod → `ipod selbst
  einhängen` (`ipod_mount`, on at first) turns it off. it needs `udisks2`,
  which nearly every distribution has.
- **ejecting** unmounts by way of udisks too and then powers the ipod off,
  so it says it may be unplugged, as after a mac ejects it. without udisks,
  `umount` as before.
- **light and dark** follow the desktop by way of the desktop portal
  (`org.freedesktop.appearance` `color-scheme`), which gnome, kde and the
  portals of sway and hyprland answer alike; gnome's `gsettings`, which nothing
  else answers, is asked only where there is no portal.
- **dragging files in** from a file manager works under x11 and not under
  wayland, where the toolkit cannot take a drop yet. menu → bibliothek →
  `ordner…` brings a folder in instead.

## connecting an old ipod

the 3rd generation (2003) syncs over both firewire and usb 2.0 but charges only
over firewire. on a modern mac that means a thunderbolt → firewire 800 adapter
plus an 800 → 400 cable, or the ipod's own power supply. if the battery is flat
the device will not mount at all over usb alone.

## not done yet

- the hash58 signature, which is what stands between kessler and the ipod
  classic / nano 3g–4g
- album artwork on the ipod (ithmb conversion)
- lists as `.m3u` files beside the music, and on the ipod
- the ipod's own playlists in the window — they are read and counted, not shown
- watching the music folders while the window is open (today they are read
  again when it opens, and on „jetzt neu einlesen")
- listenbrainz (last.fm scrobbling is done)
