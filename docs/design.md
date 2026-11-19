# kessler — design

how the window looks and moves, written down so that each new part — the
radio, the podcasts, whatever comes next — takes it over instead of making
it up again.

**the measure is the music view.** where something new has to decide how far
apart, how big, which tone, how it moves: look at how the library's columns
do it, and do the same. this file says what that is; the numbers live in the
code under the names given here, and the code is right where the two
disagree — then fix this file.

## the idea

- **like a telephone book.** order comes from the cut of the type, its tone,
  its spacing and its indent — not from lines, frames or buttons. the one line
  is the timeline. no boxes around groups, no dividers between rows.
- **one view, made as good as it can be.** no second arrangement to switch to.
- **quiet until touched.** an action word shows under the pointer, not all
  day (`archivieren`, `entfernen`, `leeren`, `aktualisieren`); a head that
  does nothing does not light up.

## type

| role | for |
|---|---|
| `Content` | rows: names, titles |
| `ContentBold` | the chosen row, a record's or episode's title |
| `Label` | heads, facts, words at a row's end, the masthead |
| `Figures` / `FiguresLabel` | numbers in the mono cut: track numbers, times, counts |

- a row's facts under its title — a record's tracks · year · genre ·
  bitrate, an episode's podcast · day, a station's land · genre — in
  `Label`, in the colour `flac` has beside a title (`RowState::secondary`,
  `faint` on an unmarked row); who made it, where it has a line of its own,
  in `Content`, the second tone.
- from a title to the first line under it: a heading's line step,
  `row · head_line()`, baseline to baseline — in the music, the radio and the
  podcasts alike. a record's head, an episode's page and a podcast's head set
  their title, the name under it and the facts the same way: title in the bold
  cut and the ink, the name in the plain cut and the second tone, the facts
  small in the third, each a heading's step under the line before
  (`podcast_col::title_lines`, as `tracks::album_head`).
- small lines one under another stand `text::SMALL_LINE` apart (the label
  size at `SMALL_LEADING`, 1.5 — a little more open than the reading size's
  prose at 1.32; small prose is set the same): a station's facts, an episode's lines, the narrow
  window's names.
- figures stay regular whatever the row does; bold digits only make a column
  uneven.

## tones (`Palette`)

one colour for each kind of word, the same in every scheme — the colours
serve the system, so a head reads as a head and a fact as a fact whichever
scheme is on:

| kind | for | tone |
|---|---|---|
| head | column heads, heads inside a column (`mitglieder`, `hören`, `heute`, `aus wikipedia`) | `heading()` |
| name | a record, a track, an episode, a member | `ink` |
| second name | the artist under a record, the podcast under an episode | `mid` |
| facts | years, counts, bitrate, day, length, `auch in …` — small | `faint` |
| reading text | wikipedia, an episode's or a podcast's description | `ink` |
| action | `abspielen`, `herunterladen`, a name in the library to click | `mid`, the ink under the pointer |
| note | empty lists, explanations in the menu | `faint` |
| what plays | the track, the episode, the station — and news | `accent` |

- every scheme names its own `heading`, apart from `faint`, `mid` and
  `accent` (the test `the_heads_stand_apart` holds it there): a head in the
  facts' colour read as one more line of facts.
- `accent` is not for decoration, nor for a view that is chosen.
- `ground`, `rule`, `field`, `hover`, `marked()` — the surfaces.
- the sharpest tone (ink, accent or mid, whichever stands out most from the
  ground: `sharpest()` in controls.rs) for marks that have to be seen on the
  timeline — the chapters' strokes and numbers. the timeline itself has two
  tones only, heard and not heard, as with music — no tint per chapter.

## rows and states (`RowState`)

- height `m.row`, its text on the row's line (`text::row_line`), air
  `ROW_AIR` 4.
- **hover**: the light plate (`hover_colour`), fading in over `HOVER_FADE`
  0.08 s. pointed at from elsewhere (`pointed`) wears it too.
- **chosen / active**: the darker plate (`marker_colour`) where the column
  has the keyboard (`selected` + `focused`); bold where it has not.
- **playing**: the arrow before the name (`lead_arrow`, sliding in over
  `ARROW_SECS` 0.16 s) and the accent — the track, the episode, the station,
  and the artist or podcast whose it is.
- **views** over the names — `alle`, `zuletzt`, `kompilationen` over the
  artists; `alle`, `offen`, `heruntergeladen`, `gemerkt` over the podcasts —
  are muted, in the second tone: they are not names. chosen, they have the
  arrow (and the darker plate where the column has the keys), never the
  accent. a chosen name is bold and on the plate, no arrow.
- **chosen** is in the ink and the bold cut in every list — a played row of
  a queue, a view over the names, a station heard alike; only a row whose
  file is gone stays in the second tone.
- the darker plate goes where the last click went: on what is chosen in the
  column clicked last, nowhere else — the others keep their arrow, accent or
  bold.
- **muted** (heard, a view's name): second tone. **spent** (played,
  archived): faint; an archived episode's picture under a veil of the ground
  (`ARCHIVED_VEIL` 0.6).
- **a choice, which way it runs decides its mark**: words side by side —
  the masthead's switch, the menu's tabs along the top, the equalizer's
  `an`/`aus` and its settings — mark the chosen one with a line under it
  (`paint_underline`), the word in the ink and the rest in `mid`, no colour
  of its own; rows down a column — names, views, the menu's tabs down the
  side — mark it with the arrow before it (`lead_arrow`). never the accent
  alone: the accent is for what plays.
- a long name scrolls under the pointer rather than ending in „…"
  (`cell_text`), fading at its ends (`EDGE_FADE` 22).
- numbers before titles in the number column (`TrackColumn::Number`, 26 wide,
  `number_gap`) — tracks of a record, chapters of an episode.
- the word at a row's end (`row_ahead`, `point_at_word`, `on_the_word`):
  the figure at rest, the action word under the pointer, a hand over it.

## columns

- **as wide as each other** at every start (`even_columns_again`); a drag on
  a gap holds until kessler is quit; a double click on a gap shares the row
  out evenly again. least widths `MIN_NAME_COLUMN` 110, `MIN_TRACK_COLUMN`
  240; `CELL_GAP` 20 between cells, `m.column_gap` between columns.
- **heads**: `Label` in `heading()`; `heading` where the head does something
  (closes, follows), `still_heading` / `still_heading_with` where only the
  word at its end does — then no plate on the name. a count at rest at the
  end becomes the word under the pointer (`leeren`). air under a head
  `HEAD_AIR` 6; between blocks `BLOCK_AIR` 8.
- **section heads inside a list or a page** (`heute`, `diese woche`,
  `älter`, `kapitel`, `aus wikipedia`, `mitglieder`, `hören`) in `Label` and
  the heads' colour (`heading()`), with a block's air before each but the
  first.
- **edges fade** instead of cutting: `FADE_HEIGHT` 28 at top and foot; under
  the last row `LIST_END_AIR` (the fade's height), so the end reads as an
  end.
- **empty**: one sentence in `faint` saying what would be there and how it
  gets there.

## movement

- a column coming or going **slides**: `slide` / `slide_for`, `FOURTH_SLIDE`
  0.22 s, eased out (`cubic_out`) — the others give it room as it comes, and
  it is drawn at its whole width, seen through the room it has so far (the
  library's fourth column, the radio's side, the podcasts' queue).
- a narrow window's strip of columns slides by its offset over 0.22 s.
- the timeline and the volume swell under the pointer over `SWELL_TIME`
  0.13 s; their knob grows out of the line.
- a name turning bold thickens over `WEIGHT_SECS` 0.12 s.
- scrolling: no bars; a flick coasts `KINETIC_DECAY` 0.15 s, which is also
  how far the rubber band stretches at a list's end.

## marks and plates

- **badge** (`badge`, `badge_centred`): ink plate, ground type, `BADGE_PAD`
  7 × 5, `BADGE_AWAY` 10 from what it belongs to, kept `BADGE_EDGE` 16 from
  the window's edges — the drag count, the volume's figure, the timeline's
  second while scrubbing, a chapter's number (compact header).
- **drop line**: a hair of 2 in the ink where something carried would land.
- **corners**: `MARKER_RADIUS` 2 for every plate.
- **pictures**: screened (dithered) as the covers are, `picture_at` for a
  square, `wide_picture` for any shape; decoded on a thread and kept small.

## the masthead

- controls `CONTROL_GAP` 28 apart, keys `KEY_SIZE` 22 on a `RING_STEP` grid;
  at rest in `mid`, under the pointer in the ink on the hover plate, on in the
  accent — every control, wherever it is (`Key`, `key_colour`).
- the switch's words in the order of their keys: `1` musik, `2` radio, `3`
  podcasts, `4` ipod. no figures beside them.

## words

- lowercase, german first (english and japanese in `src/lang`), the
  window's own words for things: `merken`, not „favorit"; `einreihen`, not
  „zur playlist".
- a setting's answer is a word, not a switch: `an` / `aus`, `ausgegraut` /
  `ausgeblendet`.
- a sentence that explains sits under the rows it is about, in `faint`
  (`explanation` in the menu).
