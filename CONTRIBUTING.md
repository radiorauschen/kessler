# contributing to kessler

thank you for wanting to make kessler better. bug reports, ideas and pull
requests are all welcome. this page says what helps and what to know before
you start.

## reporting a bug

open an issue and say:

- which kessler (menu → about) and which system: macos version, or linux
  distribution, desktop and wayland/x11
- what you did, what you expected, what happened instead
- for anything with an ipod: the model and generation
- a screenshot, if it is something you can see

## building and testing

you need a rust toolchain (`rustup`, stable). on linux also
`libasound2-dev` and `pkg-config`.

```sh
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
```

a pull request should pass all three: no failing tests, no clippy warnings.
`src/main.rs` denies dead code, so whatever a change leaves unused goes too.

## debugging

- **hold ⌥ (option; alt on linux).** some rows stay out of sight until
  the key is held, because few people need them:
  - in the menu: about → `measure`, `system fonts`, radio →
    `example stations`, and the group with `my own` colour scheme
  - in the ipod column: `save report` (everything kessler sees on the device,
    as a text file to attach to an issue) and `tidy up` (files on the ipod
    its database does not list)
  - on the green window circle (mac): zoom instead of full screen
- **measure.** about → `measure` (with ⌥) writes one line every five seconds
  to `kessler-messung.log` beside the library: frames drawn and how long they
  took, how much of a core and how much memory kessler used, the keys
  pressed, and what was going on (music, window in front or behind, menu,
  podcasts, covers loading …). turn it on, do the slow thing, turn it off,
  and attach the file. to measure from the very first frame, start with
  `KESSLER_MESSUNG=/path/to/file.log kessler gui`.
- **logs.** `kessler -v gui` logs to the terminal, `-vv` says more.
  `KESSLER_LOG` takes a filter of its own (`KESSLER_LOG=kessler::ipod=debug`).
- **from the terminal.** `kessler paths` says where the config, the library
  and the data folder are (the measurement lands there); `kessler stats` counts the library; `kessler ipod`
  looks at a connected ipod without writing anything.
- **drawing.** on linux the window draws through opengl;
  `KESSLER_GRAFIK=wgpu` draws through wgpu instead, to compare the two.
- **your own files.** `KESSLER_TRY_FILE=/path/to/file.m4a cargo test
  a_real_file -- --nocapture` tells whether a file would play and where it
  goes wrong. `./bundle/smoke-tags.sh` reads and writes tags in every format.
- **without a screen.** `Xvfb :77 -screen 0 1400x700x24`, then
  `DISPLAY=:77 HOME=/tmp/k ./target/release/kessler gui` (its own `HOME`, so
  your config stays untouched), a picture with `import -window root shot.png`.
  keys only arrive after `xdotool windowfocus --sync <window>`. there is no
  sound there: what is heard is checked through tests.

## where things are

| folder | what lives there |
|---|---|
| `src/gui/` | the window: `mod.rs` (the app and the frame loop), `app/` (actions, one file per topic), `library/` (drawing the columns), `menu.rs`, `theme.rs` (colour schemes, measures), `text.rs` |
| `src/player/` | playback: reading ahead, gapless, the equaliser, radio streams |
| `src/library/` | importing music, tags, cover art |
| `src/db/` | the sqlite library |
| `src/ipod/` | the ipod database, syncing, play counts |
| `src/radio/`, `src/podcast/`, `src/lastfm/` | what their names say |
| `src/media/` | media keys, the menu bar, mpris |
| `src/lang/` | the english and japanese words |
| `docs/` | the manual and the design rules |

most files start with a `//!` comment that says what they are for, and the
numbers and functions carry `///` comments that say why they are the way
they are. read those first.

## things that are easy to trip over

- **three languages.** every word the window shows is written in german in
  the code, as `tr!("…")` — a literal, with bare `{}` for what goes in. the
  english stands in `src/lang/en/`, the japanese in `src/lang/ja/`, keyed by
  the german. the tests fail when one is missing. if you don't speak german
  or japanese, write what you can and say so in the pull request; the rest
  will be filled in.
- **lower case.** the window writes everything in lower case, in all three
  languages.
- **design.** read [docs/design.md](docs/design.md) before you change how
  something looks. the short version: like a telephone book — order from the
  cut, tone, spacing and indent of the type, no lines, frames or buttons; the
  music view is the measure for everything else.
- **options** go into the menu and into `[ui]` of the config, with
  `#[serde(default)]` so older configs still load.
- **comments** are english prose and explain *why*, not what.
- **typefaces.** only add a typeface whose licence lets it travel with
  kessler.

for anything bigger — a new view, a new dependency, a change to how
something looks — open an issue first, so we can talk about it before you
spend the time.

## how a pull request comes in

kessler is developed in a private repository and published here with each
release, and every merge here builds new packages at once. so a pull request
is not merged here; instead:

1. once it is good to go, it is taken over into the development repository,
   and you get a note on the pull request saying so.
2. it goes out with the next release. you are named as co-author of that
   release's commit — so it counts for you on github — and thanked by name
   in the release notes.
3. the pull request is then closed with a link to that release.

so a closed pull request here does not mean it was turned down.

## licence

kessler is under the [gnu general public license](LICENSE), version 3 or
later. by sending a contribution you agree that it is under the same terms.
the drawings have their own licence ([LICENSE-drawing.md](LICENSE-drawing.md))
and are not open for changes.
