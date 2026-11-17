# changes

what changed from one version to the next, newest first. a test checks that
the topmost version is the one being built.

a line starting with `## ` is a version; one starting with `- ` is a change.
everything else is skipped.

## 1.5.248

- **radio** steadier: a station is heard once two seconds of it have come in, and up to eight seconds are kept ahead; should the network still run dry, it waits for two seconds again — one pause rather than a stutter
- **record heads** in the combined view: album, artist and the facts (number of tracks, year, genre, added, bitrate) on three lines beside the cover, or two beside a small one; from 140 pt the lines stand under the cover
- **track list**: a new column „bitrate" (sortable, off at first); with more than one album in the list, each gets a small heading of its own
- **podcasts and radio**: the same line spacing as the music; the facts of an episode and of a station in the colour of „flac"; the picture of an episode a little larger
- **cover size** slider in proportions rather than points, so the small sizes get more room; first start with 85 pt and the bitrate shown
- **app icon**: the rounder corners of current macos, a glassy edge, a lighter shadow, the k in its optical centre; schemes with a grey ground take a stronger colour of their own, so that icons are easier to tell apart
- **small things**: reading the library at start shows quietly at the bottom right, and messages there fade more slowly; in the narrow window the columns reach further down
- **measuring**: the radio writes when it connects, reconnects or breaks off, when its store ran dry and when the sound comes in slower than the station's bitrate; the brackets name only what wanted the next frame soon

## 1.5.223

- **new version**: on the mac it opens the newest signed and notarised dmg directly instead of the release page
- **mac download**: one fixed link that always points to the newest notarised dmg (see the readme)

## 1.5.222

- **podcasts, queue** as in the music: played episodes stay above it, faint, a double click jumps back or ahead, a cursor of its own for the keyboard (j/k, d, shift + j/k, shift + d twice empties it, shift + arrow marks several), every episode only once; an episode still loading keeps its place and says „lädt…"
- **ipod, experiment**: downloaded episodes not yet heard as a playlist „podcasts" on older ipods; heard ones leave again at the next sync
- **ipod sync** sturdier: the database is written in full before it replaces the old one; a missing folder fails only that track
- **radio**: the line stays calm while connecting („sender verbinde…" with running dots), changes cross-fade, dot and „live" glide; with muto the animation starts as a square that rounds into a circle at the end
- **menu bar (mac)**: sleep timer in the menu while an episode plays (also in the linux panel); cover above the title with an even margin all round
- **smoother**: no short stutter every 15 seconds during an episode any more; fewer redraws while an episode streams; loudness levelling measures several records at once
- **small things**: a large soft shadow under the context menu; „bei last.fm lieben" in the context menu; system cursor on the column divider; number of artists at the top right (menu → künstler:innen); arrows no longer run on when switching back; the rest of a mouse-wheel step no longer spills into the next column; „windpark soft" under „über" is a link
- **fixed**: very large podcast feeds would not load; changing episodes briefly showed the music title at the top
- **measuring**: shows the memory now and what the podcasts do (feeds, views, loading)

## 1.5.200

- **equalizer** (`z`): a column of its own in music, radio and podcasts — ten bands, presets, two of your own; lowers itself automatically so that nothing clips
- **views**: music, radio, podcasts and ipod can each be switched on and off (menu → allgemein, or `c` and a digit); on the first start everything is off and the window says how to switch on; every view remembers its own side column
- **keyboard**: radio and podcasts entirely by arrow keys, the context menu with `v`, the cursor in the search field with ← →, key overview (`?`) rearranged
- **podcasts**: feeds are only asked whether anything changed; series by season and episode; sleep timer in the footer; much less memory
- **radio**: starts without a station; a station without a connection no longer holds up the window and is tuned in again
- **playback**: after a pause or a lost output device it goes on at the same place; a crashed decoder no longer takes anything down with it
- **pictures**: a sample picture under darstellung → bilder that shows grid, tones and fineness at once; pictures in the search field reach the edge; kessler saga with an opening and more landscape
- **mac**: draws directly through metal (less memory), a square symbol in the menu bar, a new icon with edge and shadow, a .dmg with its own picture, signing and notarising by script
- **linux**: mouse wheel and title bar as on the mac, vulkan if wanted (`KESSLER_GRAFIK=wgpu`)
- **menu**: „tasten", „neue version" and „sparsam" under allgemein; rating as stars; the system clipboard; a new default colour scheme 公園, light or dark as the system is
- kessler collects nothing: no telemetry, no analytics, no crash reports — now also stated in the readme and the manual

## 1.5.116

- **podcasts**: episodes by feed address or search, new episodes in the background, downloading, listening on where you left off, keeping, archiving and deleting per episode, chapters on the timeline, a queue of its own (`w`), the show's description and links (`i`), „offen" for what has not been heard, sharing, a sleep timer to the end of the episode
- **radio**: stations by stream address, search by genre, country and bitrate, what plays next where the station says, calmer empty states
- **menu** rearranged: tabs of their own for allgemein, radio and podcasts; radio, podcasts and ipod can be switched off entirely; „einstellungen zurücksetzen"
- **masthead**: new switches between music, radio, podcasts and ipod, the menu button in several shapes, every control by one colour rule
- **pictures**: a new grid „blue noise", newly chosen grid colours; smaller pictures and a bounded picture store, much less memory
- **search field and screen saver**: kessler saga, ball, zarathustra in misaki as running text, a lyrics saver with the sung line in the middle
- **playback**: rodio 0.23 — no more clicks between tracks of different sample rates, no distorted voices when changing between music, radio and podcast, seeking without hanging
- **other artists** as a tree, left-aligned, brackets in the colour of their names
- menu bar (mac) with the cover of the playing episode; junicode and chivo mono are out again; many smaller fixes

## 1.4.134

- the first public version of kessler: a music player with ipod management for the mac and linux, with radio, last.fm, lyrics, information about artists and three languages
