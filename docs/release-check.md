# before a release

what is gone through before a version is put out, from the first release
candidate on. while a candidate stands, nothing new goes in: only what is
broken is mended, each mend a patch version of its own.

## by machine

run from the project folder; all of it has to come back clean.

- `cargo test` — every test passes.
- `cargo clippy --all-targets` — no warnings.
- the same for the mac, from linux: `cargo clippy --target x86_64-apple-darwin
  --all-targets`, with a `cc` that swaps the c needing apple's frameworks for
  an empty file, and `--cfg kessler_tray --cfg kessler_media` in the rustflags
  so the menu bar and the media keys are checked too.
- `./bundle/smoke-tags.sh` — editing tags on real files, the 18 odd and
  broken ones among them: each is edited cleanly or left byte for byte.
- an ipod end to end: the release binary with a home of its own, a few tracks
  made with ffmpeg, a third-generation ipod built in a folder, `kessler scan`
  and `kessler ipod sync --apply`, then `kessler ipod report` and a reading of
  the written `iTunesDB` (indices, ascending track ids, the master list in
  shelf order).
- an old config: one from before the candidate, with keys that have since
  gone (`recent_count`, `source_switch`, `clock`, `arrows` …), loads, keeps
  what is still read, and is written back without losing anything.
- the window under xvfb: each view (musik, radio, ipod; kompakt, super
  kompakt, narrow), the menu on every tab, the key sheet, in a light and a
  dark scheme, at 1× and 2× — looked at for anything cut off, overlapping or
  out of line.
- the words: every word on screen in german, english and japanese (the tests
  check the tables), and a look through the english and japanese windows for
  anything too long for its place.
- the manual and the readme against the menu: every row the menu has is
  described, nothing described is gone.
- the changelog: its top entry is the version, written for the person who
  uses kessler.

## by hand, on the mac

what only the real machine can show.

- first start with no config and no library: the folder is asked for, the
  scan runs, the window opens on `zuletzt`.
- play, pause, skip, scrub, volume; a record through, gapless; shuffle and
  repeat; the queue dragged about; the media keys and the menu bar.
- the tag editor on a few files, and the files read back.
- the ipod: plugged in, the column, a selection dragged onto `ipod`,
  `synchronisieren…`, ejected, and the menus on the ipod itself.
- radio: a few stations, one that drops, pause and back, `gerade gehört`,
  `merken`, last.fm.
- last.fm: a track scrobbled, one while offline sent later.
- the window: zoomed by double click on the band, narrowed, full screen,
  minimised and brought back from the menu bar.
- light and dark, the system following; a few schemes.
- the log (`log ansehen`) after an hour of use: nothing red that was not
  expected.

anything found goes in a note with what was done, what was seen and what was
expected; a screenshot if it is something to see.
