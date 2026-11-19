# changes

what changed from one version to the next, newest first. a test checks that
the topmost version is the one being built.

a line starting with `## ` is a version; one starting with `- ` is a change.
everything else is skipped.

## 1.5.273

- **colours as a system**, the same in every view and every scheme: heads (column heads and the heads inside a column — „aus wikipedia", „mitglieder", „kapitel", „heute", the menu's) in a colour of their own; reading text — wikipedia, a station's, a podcast's and an episode's description — in the names' ink; small facts (years, dates, counts, bitrate) in the third tone; what can be clicked in the second tone; the accent only for what plays
- **head colours**: every scheme now names its own, clearly apart from the facts, the second tone and the accent — 27 schemes had none and set their heads like the facts; they take a colour from where they come from (solarized's violet, nord's aurora, adwaita's purple, dracula, gruvbox …); runebending's heads in lilac; hudson's and coco壱番屋's no longer look like the accent; a test keeps every scheme that way
- **chosen** is in the ink and bold in every list — a played row of the queue and a chosen view too, in music, radio and podcasts; the views over the podcasts („alle", „offen", „heruntergeladen", „gemerkt") are set like „alle", „zuletzt", „kompilationen" over the artists, and a podcast has the arrow and the accent only while its episode plays
- **episode page**: what can be done to it in three groups under small heads — „hören", „datei", „liste" — side by side where the column is wide enough, else one under another; „datei löschen" also while it plays, and „herunterladen abbrechen" for a download under way or waiting (also in the right-click menu)
- **title, name and facts** set alike everywhere: an episode's page and a podcast's head set their title, podcast or makers and facts as a record's head does — the same cuts, tones and steps
- **members**: a former member in the same colour as the current ones, with „(ehemalig)" after the years; the second tone stays for names in your library that can be clicked
- **english**: an episode's „merken" is „bookmark" now, not „keep"

## 1.5.266

- **podcasts, search**: the search field now also searches the podcasts you follow and their episodes, at once as you type — by name, author, title and description; the first column shows „in deinen podcasts" (the podcasts something was found in, each with how many episodes match, and „folgen" to list all matching episodes) and, below it, „neue podcasts" from the directory; the episodes column narrows to what matches and counts it
- **podcasts, download**: episodes can be downloaded without playing them — „herunterladen" on the episode's page, in its right-click menu, or for several marked at once; they download one after the other in the background („lädt herunter 34 %") and then stand under „heruntergeladen" (formerly „geladen")
- **podcasts, kept**: an episode downloaded by hand stays after it has been heard and is not cleared by „heruntergeladene löschen nach"; only deleting it, or archiving it with „archivierte dateien löschen" on, lets it go
- **podcasts, keyboard**: a podcast in the first column has a menu of its own — right click, or `v` on the one chosen — with „folgen" / „nicht mehr folgen", so a podcast found can be followed without the mouse
- **record heads**: when the details do not fit beside the cover they wrap onto a third line; a new option „unter dem namen" (menu → ansicht) brings back the old single line; first start with 80 pt covers
- **last.fm** at the bottom right says what the time means („last.fm zählt in 2:10", then „gezählt") and hides while paused; on the radio a pause no longer counts as listening
- **small type** with a little more line spacing: episode and podcast descriptions, station details, notes in the menu
- **small things**: in the radio the columns reach down into the foot as in the music; „beim öffnen zeigen" indented under the row it belongs to; the bottom right no longer shows the music search's counts while searching podcasts
- **fixed**: on the mac at a zoom below 100 % the search field in „super kompakt" slid onto the window buttons; in the combined view `gg` (or `k` onto a record's first track) left the cover half hidden

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
