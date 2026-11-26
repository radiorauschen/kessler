use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{KesslerError, Result};

/// Top-level application configuration, loaded from `~/.config/kessler/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Directories to scan for music files.
    pub music_dirs: Vec<PathBuf>,
    /// Whether the music folders are read again every time the window opens
    /// („beim start einlesen" in the menu).
    pub watch_dirs: bool,
    /// Whether music dropped on the window is moved into the library rather
    /// than copied into it. Off: emptying somebody's download folder is not a
    /// thing to do by default.
    pub add_move: bool,
    /// Whether newly added music is handed to Music.app as well. Off, and a
    /// macOS idea — elsewhere it does nothing.
    pub add_to_music: bool,
    /// Whether what an iPod cannot play — FLAC, above all — is converted to
    /// MP3 on its way there when it is synced. On: the plan says so before
    /// anything is copied, and it needs ffmpeg („flac umwandeln" in the
    /// menu's ipod section).
    pub ipod_convert: bool,
    /// What it is converted to: „aac 256" (the default), „aac 320", „aac 192",
    /// „alac" — lossless, for an iPod that plays it — or „mp3 320". See
    /// `ipod::sync::Target`. „umwandeln in" in the menu's ipod section.
    pub ipod_format: String,
    /// What happens when an iPod is plugged in: „nichts", „fragen" — the
    /// overview of what a sync of the whole library would copy, as
    /// `überspielen` opens it — or „abgleichen", the same sync without asking.
    /// Also when the window opens with one already plugged in.
    pub ipod_on_connect: String,
    /// On Linux: mount an iPod nobody has mounted, by way of udisks, as a file
    /// manager would — under a tiling window manager there is often none to do
    /// it. „selbst einhängen" in the menu's ipod section.
    pub ipod_mount: bool,
    /// Whether what an iPod played and rated while it was away comes into the
    /// library — and to last.fm, while it is connected — when it is plugged in.
    /// On.
    pub ipod_take_plays: bool,
    /// An experiment: the podcast episodes fetched and not yet heard go onto
    /// the iPod with a sync of the whole library, and stand there as a
    /// playlist „podcasts", the newest first — the old iPods have no podcasts
    /// of their own. Off.
    pub ipod_podcasts: bool,
    /// Database file location. Defaults to `~/.local/share/kessler/library.db`.
    pub database_path: Option<PathBuf>,
    /// UI configuration.
    pub ui: UiConfig,
    /// Player configuration.
    pub player: PlayerConfig,
    /// Last.fm, if it is connected.
    pub lastfm: LastfmConfig,
    /// Where a config file that could not be read was put aside, this run —
    /// for the window to say so. Never written.
    #[serde(skip)]
    pub set_aside: Option<PathBuf>,
}

/// What Last.fm needs to know who is listening.
///
/// The key and the secret belong to *an application*, and this program does not
/// ship one: they are made at last.fm/api in a minute and written here by
/// `kessler lastfm setup`. Nothing secret is in the source, and the program
/// stays something you can pass on.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LastfmConfig {
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    /// Written by `kessler lastfm login`. It does not expire; deleting it
    /// disconnects the account.
    pub session_key: Option<String>,
    /// Who that session belongs to, for the interface to say.
    pub username: Option<String>,
    /// Whether to keep the "listening now" line on the profile up to date.
    /// Scrobbling itself is on whenever an account is connected — that is what
    /// connecting one is for.
    pub now_playing: bool,
}

/// `[ui] music` in a file that does not mention it: one written before the
/// music could be switched off, by somebody who used it.
fn music_where_unsaid() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// The colour scheme, by name — "systemfarben",
    /// "macos hell", "nord" and so on. Names of schemes that have since been
    /// taken out are still understood and open on the nearest native one.
    pub ground: String,
    /// Which fields the track list shows, besides the
    /// title. Empty or unreadable falls back to the default.
    pub track_columns: Vec<String>,
    /// Show how many tracks each artist has. Off by
    /// default — it is a figure you look for occasionally, not one you read.
    pub artist_counts: bool,
    /// A register in the artist column: the first letter of the names,
    /// small, over each run of them, as the podcasts' „heute" and „diese
    /// woche" stand over theirs — only while the column is in alphabetical
    /// order. On.
    pub artist_letters: bool,
    /// The letter bar: the register's letters in the gap beside the artist
    /// column, while the pointer is there; a letter brings its names to the
    /// top. A beta, off.
    pub artist_letter_bar: bool,
    /// Whether the head of the artists' column says how many artists there
    /// are, at its right — as the podcasts' column says how many it holds.
    /// Off.
    pub artist_total: bool,
    /// Whether the albums stand in the track list, each as a heading with its
    /// cover over its own tracks, instead of in a column of their own. On for a
    /// fresh install — the way the window should first look.
    pub albums_in_tracks: bool,
    /// A compilation under one of its artists: the whole record, the other
    /// artists' tracks dimmed around the chosen one's, or only the chosen one's
    /// tracks.
    pub compilations_whole: bool,
    /// Under „kompakt" and „super kompakt": what the library holds, flush left
    /// at the foot — or not.
    pub library_line: bool,
    /// The „compilations" row under „zuletzt" in the first column.
    pub show_compilations: bool,
    /// Whether the header offers „radio". Off, nothing of the radio is read,
    /// held or fetched — see `crate:radio`.
    pub radio: bool,
    /// Whether kessler looks for an iPod at all. Off, nothing is watched for
    /// and its tab is gone. On.
    pub ipod: bool,
    /// Whether the podcasts are on offer: the row under „über" that opens their
    /// column. Off, only with ⌥ held. Off.
    pub podcasts: bool,
    /// Whether the music is on offer: off, its word leaves the switch, the
    /// library is not read at the start, and nothing starts a track. Off on a
    /// first start, with nothing switched on; the music was all there was then.
    #[serde(default = "music_where_unsaid")]
    pub music: bool,
    /// Names with their capitals, as written, instead of everything small. Off.
    pub capitals: bool,
    /// On a MacBook with a Touch Bar, what plays and the keys to steer it
    /// there while the window is in front. On.
    pub touchbar: bool,
    /// The picture there beside what plays: „suchfeld" (the default, the
    /// search field's, whatever it is), one of the field's pictures by its
    /// name — „kessler saga", „zeiger", … — or „aus".
    pub touchbar_view: String,
    /// The picture filling the whole bar beside the cover, a touch on it
    /// playing or pausing, rather than beside what plays and the keys. Off.
    pub touchbar_whole: bool,
    /// Pause when the sound would go on out of the built-in speakers once
    /// the headphones are gone — AirPods out, a cable pulled. A Mac only.
    /// On.
    pub pause_unplugged: bool,
    /// How far „zurück" and „vor" skip in an episode, in seconds. 10 and 10.
    pub podcast_back: u32,
    pub podcast_ahead: u32,
    /// How long an episode fetched is kept: „gehört" — until heard to its end,
    /// as until then —, so many days, or „immer".
    pub podcast_keep: String,
    /// Whether the episodes archived stand in the lists, greyed, or are left
    /// out; and whether archiving one lets go of its file.
    pub podcast_show_archived: bool,
    /// How often new episodes are looked for: every so many minutes — „15",
    /// „30", „60", „180" — or „hand", only when asked (menu → podcasts, or
    /// „aktualisieren" at the podcasts' head).
    pub podcast_check: String,
    /// Whether the podcasts' queue stands as their fourth column, where there
    /// is room for it — `w` there.
    pub podcast_queue: bool,
    /// Whether „informationen" stands beside the radio, as it was left there.
    pub radio_info: bool,
    /// How the podcasts followed are listed: „zuletzt" — whose episode came out
    /// last first, „a-z" or „z-a".
    /// A register in the podcasts' first column, as in the artists': only
    /// while it is in alphabetical order. On.
    pub podcast_letters: bool,
    pub podcast_order: String,
    pub podcast_archive_forgets: bool,
    /// What a station says it plays goes to last.fm like any track.
    pub radio_scrobble: bool,
    /// A song kept from the radio is loved on last.fm as well.
    pub radio_love: bool,
    /// The equalizer: on or off, and its ten bands in decibels, from 31 Hz up —
    /// see `player::eq`. Off and flat to begin with.
    pub eq_on: bool,
    /// The two settings of one's own, as `eq_gains`; empty while nothing is
    /// kept there.
    pub eq_own_1: String,
    pub eq_own_2: String,
    /// Which own setting the bands are kept in as they move: 1 or 2, or 0
    /// for none (a ready-made one, or nothing chosen).
    pub eq_slot: u8,
    /// How far a notch of the mouse wheel scrolls: „langsam", „normal" or
    /// „schnell" — see `gui::library::wheel_step`.
    pub scroll_speed: String,
    pub eq_gains: String,
    /// Stations' pictures are fetched.
    pub radio_logos: bool,
    /// The cover of the song a station plays is looked up by artist and
    /// title (in the iTunes store's search) and shown under „informationen"
    /// where the artist has no picture of their own. On.
    pub radio_song_covers: bool,
    /// What stands beside a station a search found: „rechts" — its first
    /// genre, land and bitrate small at the row's end, as „flac" stands
    /// beside a title; „spalten" — the three in columns of their own, with
    /// heads, where the column is wide enough; „aus" — the name alone.
    /// The order of the stations kept: „eigene", as they were dragged, or
    /// „a-z" / „z-a" by name.
    pub radio_order: String,
    /// A register over the stations kept, as in the artists': only while
    /// they are in alphabetical order. On.
    pub radio_letters: bool,
    pub radio_found: String,
    /// Whether the first column has its „zuletzt" row at all, and the lists
    /// their newest-tracks row. Off, the window opens on the whole library; the
    /// iPod's „zuletzt hinzugefügt" list stays, at the same length.
    pub show_recent: bool,
    /// Whether the artist column keeps its "alle" row — the one that means
    /// "every artist". Off: the window already opens on the whole library, and
    /// the row spends a line saying so.
    pub show_all_artists: bool,
    /// Whether the window opens on „zuletzt" — the newest records — rather
    /// than on the whole library. On by default: what came in last is what
    /// somebody opening the program most often wants to hear.
    pub start_on_recent: bool,
    /// Whether the lists have the fourth column, in place of the queue.
    /// `u` swaps them in, `w` swaps the queue back.
    pub show_lists: bool,
    /// Where what is playing stands, by name: „zeile" (its own line under the
    /// header), „cover" (the same with the record's cover beside it) or
    /// „kompakt" (everything on one line). Anything else — including the places
    /// that have since been taken out — is „zeile". A fresh install opens on
    /// „kompakt"; a config that already says something keeps it.
    pub playing_at: String,
    /// The screen saver: after how many minutes without a touch it comes
    /// up, 0 for never; how big its dots are, in device pixels (1 … 3); and
    /// how long its afterglow is (0 short, 1 middle, 2 long).
    pub saver_after: u32,
    pub saver_dots: u8,
    pub saver_glow: u8,
    /// How strongly it is drawn (0 gentle, 1 middle, 2 strong).
    pub saver_strength: u8,
    /// How big the figure is (0 small, 1 middle, 2 large, 3 the whole
    /// window) — see `saver::SIZES`.
    pub saver_size: u8,
    /// Whether it also comes while nothing plays, drawing vibrations of its own
    /// that nobody hears.
    pub saver_silent: bool,
    /// How it is screened, by the names `dither_method` uses. Its own, and
    /// „geordnet" to begin with: a screen that carries its error forward
    /// reshuffles every dot of a moving picture every frame, and what should
    /// drift shimmers instead.
    pub saver_dither_method: String,
    /// What colour its dots are: 0 the accent, as it always was; 1 one colour
    /// wandering slowly through the scheme's own; 2 two of them at once, a
    /// gradient across the figure that turns as it wanders.
    pub saver_colours: u8,
    /// What it shows: „vektorskop", or „liedtext", the words of the song
    /// filling the window — the figure where a song has none. See
    /// `gui::saver::PICTURES`.
    pub saver_picture: String,
    /// The symbol in the macOS menu bar: „aus", or „k", the app's icon made
    /// small, the k cut out of a rounded box — see `gui::menubar::Mode`. Off to
    /// begin with. „skop" and „laufschrift" and „still" or „bewegt" read as
    /// „k"; `menubar_mark`, `menubar_box` and `menubar_title_size`, which an
    /// old config may still hold, nothing reads.
    pub menubar: String,
    /// One's own scheme, „eigenes", as the code of its four colours — ground,
    /// names, what plays, heads — see `gui:theme:Seed`. Empty until somebody
    /// makes one.
    pub custom_colours: String,
    /// Where that menu shows the cover of what plays: „daneben" (beside the
    /// title), „oben" (above it, as wide as the menu) or „aus" — and whether it
    /// is screened as the window's pictures are.
    pub menubar_cover: String,
    pub menubar_cover_screened: bool,
    /// The symbol's colour in a Linux panel, which has no template images:
    /// „system" — light when the system is dark —, „hell" or „dunkel". A Mac's
    /// menu bar colours it itself.
    pub menubar_ink: String,
    /// The window's language: „de", „en", or empty for the system's.
    /// A config written before there was a choice is read as „de" — see
    /// `Config::load` — so nobody's window changes language under them.
    pub language: String,
    /// How big the whole window is drawn, as a factor on what the system
    /// says a point is: 1 is the system's own size. For a screen whose
    /// system gets it wrong — a small laptop under X11 without a scale set
    /// works one out from the panel's millimetres, and can make everything
    /// half again as big — and for a desk far from the eye.
    pub zoom: f32,
    /// „system", „hell" or „dunkel": which half of the scheme in `ground` is
    /// painted. See `Brightness::from_config`.
    pub brightness: String,
    /// Whether the combined album and track column shows the track number
    /// and the length — its own switches, apart from the plain track
    /// list's.
    pub merged_number: bool,
    pub merged_length: bool,
    /// Whether each record's heading in the combined column says its
    /// bitrate, „320 kbit/s" or the span its tracks have. In the heading
    /// rather than the rows: there is no head over them to name a bare
    /// figure.
    pub merged_bitrate: bool,
    /// Whether the track numbers stand right-aligned, their digits lined up
    /// down the column — rather than at the row's left edge.
    /// Off: left.
    pub numbers_right: bool,
    /// Whether the track list's length stands at the label's size, as the times
    /// beside the radio's „gerade gehört" do, rather than at the title's. Off:
    /// as big as the title.
    pub small_length: bool,
    /// Whether every one of the iPod, the information, the words, the others
    /// and the log can stand open at once, each in a column of its own right of
    /// the queue, rather than one of them at a time. Only in a window wide
    /// enough for the side to have a column; then also in the view with both
    /// name columns. See `GuiApp::all_sides`.
    pub all_columns: bool,
    /// How the library marks a track that is on the plugged-in iPod already:
    /// the word „ipod" beside its title, as „flac" stands there, a dot, or not
    /// at all — see `library::rows::DEVICE_MARKS`.
    pub device_mark: String,
    /// On Linux: a notification for each new track while the window is not in
    /// front — title, artist and record, and the cover.
    pub notify: bool,
    /// Whether kessler asks GitHub at every start whether a newer version is
    /// out, and says so at the foot if one is. Off: nothing goes out unasked.
    pub update_on_start: bool,
    /// Whether a record's heading in the combined column says how many tracks
    /// it has. On.
    pub heading_count: bool,
    /// Whether a record's heading in the combined column is the plain one:
    /// the name, and under it one line in the reading size and the second
    /// tone — artist, year, genre, tracks, bitrate, run on with dots —
    /// rather than the stepped one, whose lines and facts the cover's size
    /// sets. Off.
    pub heading_plain: bool,
    /// Whether repeat and shuffle stand at the top of the queue's column
    /// rather than in the header beside the transport.
    pub modes_in_queue: bool,
    /// What „informationen" shows about an artist, part by part: the picture,
    /// the facts (kind, origin, years, style, MusicBrainz's note), the text
    /// from Wikipedia, what of them is in the library and on the radio, the
    /// members or groups, and who they are like. All on.
    pub info_picture: bool,
    /// Whether that picture, and a station's, is shown in its own shape as
    /// wide as the column, no taller than a quarter more than that, rather
    /// than as a square of 176 points at most. Off.
    pub info_picture_wide: bool,
    pub info_facts: bool,
    pub info_wikipedia: bool,
    pub info_library: bool,
    pub info_ties: bool,
    pub info_similar: bool,
    /// The lyrics: at content size or at label size, and whether timed words
    /// scroll along with the song. Normal, along.
    pub lyrics_size: String,
    pub lyrics_follow: bool,
    /// The log's kinds left out of its column, by their words („radio" …).
    /// None.
    pub log_hidden: Vec<String>,
    /// How many of the other artists the column lists; 0 for all of them.
    pub others_shown: u32,
    /// Whether the other artists stand as a tree: each one, braced, those of
    /// the library who name them, and braced again whom else each of those
    /// names. Off: the list.
    pub others_tree: bool,
    /// The order the artist column was last left in: „a-z", „z-a" or
    /// „zuletzt". Kept so the window opens the way it was closed.
    pub artist_order: String,
    /// The same for the album column, which also knows „jahr" and
    /// „jahr rückwärts".
    pub album_order: String,
    /// And for the track list: the field and the direction, as „year desc".
    pub track_order: String,
    /// Whether the search field carries the music as dots while something
    /// plays. Whoever wants it switches it on.
    pub field_spectrum: bool,
    /// Which picture, while it is on: „vektorskop" (twenty frames a second, see
    /// `scope_rate`), or one of the quiet ones that move only on the ticks of
    /// the time under the title — „schlange", „mandelbrot", „leben", „kugel",
    /// „zarathustra", „kessler saga"; see `gui::library::FieldPicture`.
    pub field_picture: String,
    /// Where „zarathustra" in the field has read to: a byte into the book, kept
    /// so it reads on from there next time.
    pub book_at: usize,
    /// The most frames a second the window draws, 0 for as many as the screen
    /// shows. A Mac with a 120 Hz screen drew every move of the pointer twice
    /// as often as anybody can follow, at a third of a core.
    pub frame_cap: u32,
    /// How many frames a second the scope in the search field (and the one at
    /// the foot of the menu) is drawn with while the window is in front: 20, 12
    /// or 8; 4, long-exposed; or 1, long-exposed and in step with the clock,
    /// which then moves the rule once a second too. Every one is the whole
    /// window redrawn, so this is most of what kessler costs while it is looked
    /// at.
    pub scope_rate: u32,
    /// How many steps a second the quiet pictures in the search field take, all
    /// alike: 1, 4 on the clock's own ticks (no frame more), 5, 6 or 8 on ticks
    /// of their own.
    pub field_rate: u32,
    /// How many steps a second the needles of „zeiger" take: 24, 12, 8 or
    /// 4, all on the clock's quarter seconds. 12.
    pub meter_rate: u32,
    /// Whether pictures are drawn as a halftone in the ink rather than in their
    /// own colours, and how: every cover and the artist's picture alike, set
    /// under darstellung → „bilder". Until then the headings and the cover
    /// beside what is playing each had their own three, and the artist's
    /// picture followed these with nothing in the menu to set them. On by
    /// default: it is what makes the records look like this program rather than
    /// like every other one.
    pub cover_dither: bool,
    /// A hairline round every picture, one pixel of the screen: "aus",
    /// "zart" (the third tone), "tinte" (the ink) or "akzent" (the receded
    /// accent the drawings are in). Off.
    pub picture_outline: String,
    /// The window's three buttons on a Mac: "system" (the Mac's own), "drei
    /// farben" (circles in the scheme's accent, heads' colour and second
    /// tone) or "einfarbig" (all three in the third tone, the accent under
    /// the pointer). One colour.
    pub window_buttons: String,
    /// The window's title bar on Linux: "system" (the desktop's own, or
    /// GNOME's look where the desktop draws none) or "ohne" (none at all —
    /// for a tiling window manager; the window then moves by the empty parts
    /// of the masthead's band and grows from its edges). The desktop's own.
    pub window_bar: String,
    /// The keys the lists move by: "vim" (j k h l, gg G — always there) or
    /// "emacs", which adds Emacs's own on top: ctrl + n p f b, ctrl + v and
    /// alt + v, alt + < >, ctrl + s, ctrl + g. Vim's.
    pub keymap: String,
    /// A change of view — the music, the radio, the podcasts, the iPod —
    /// comes up out of the ground, as the menu does, rather than at once.
    /// On.
    pub view_fade: bool,
    /// A column with nothing written under it at the foot runs down to the
    /// window's very edge, without a fade there, rather than stopping a
    /// margin above it. On, to try.
    pub columns_to_edge: bool,
    /// A clock at the foot's right end: „aus", „uhr" (the time) or „datum"
    /// (the date and the time). Off.
    pub foot_clock: String,
    /// How the dots are laid down: "geordnet", "halbton", "streuung",
    /// "atkinson" or "rauschen". An unknown name falls back to the first.
    pub dither_method: String,
    /// How fine the dots are, from 0 (a mosaic) to 1 (nearly the picture
    /// back). The middle is what the program did before there was a choice.
    pub dither_fineness: f32,
    /// Screened in two tones, ink and ground, or in three, with a second colour
    /// from the scheme between them. 2 or 3.
    pub dither_tones: u8,
    /// How big the cover on a record's heading is while the albums are in the
    /// track list, in points. 48 is what it was when there was no choice.
    pub heading_cover_size: f32,
    /// Whether the headings carry a cover at all. Off, a heading is its two
    /// lines of words — the record's name, and who, when and how many.
    pub heading_covers: bool,
    /// How much of the row each column takes, once it
    /// has been dragged — as a *share* of the row rather than a width in
    /// points. Unset means "work it out from the window", which is what they
    /// do until somebody moves one.
    ///
    /// A share, because a window is a thing people resize. Points were kept
    /// here first, and made the window stop being responsive the moment
    /// anything was dragged: every column held its size and the track list,
    /// which takes whatever is left, absorbed the whole loss until there was
    /// nothing left of it.
    pub artists_share: Option<f32>,
    pub albums_share: Option<f32>,
    /// And for the queue, which stands to the right of the track list. Unlike
    /// the name columns it has a width of its own rather than taking what is
    /// left: the track list is the one whose content has no natural length, so
    /// it keeps the remainder.
    pub queue_share: Option<f32>,
    /// Whether the combined view (`albums_in_tracks`) deals its row out
    /// evenly — two columns half each, three a third, four a quarter — and
    /// keeps it so as columns come and go. On until a divider is dragged
    /// there; a click on a divider, or a fourth column arriving, turns it on
    /// again. While it is on, the shares below are not read in that view.
    pub merged_even: bool,
    /// And for the side column, right of the queue, which the combined view
    /// gives the iPod, the information and the log. Unset: as wide as the
    /// queue.
    pub side_share: Option<f32>,
    /// The radio's columns as dragged: the stations' share of the row, and the
    /// side's share of what is left of it. Unset: as wide as the artists, and
    /// half.
    pub radio_stations_share: Option<f32>,
    pub radio_side_share: Option<f32>,
    /// The station's share of the room it and the programme have; unset, half.
    pub radio_page_share: Option<f32>,
    /// The podcasts' columns as dragged, each but the last its share of the
    /// row; none, or 0, every column the same.
    pub podcast_shares: Vec<f32>,

    /// Whether the queue column is shown at all.
    pub show_queue: bool,
    /// Where the window was and how big, last time it was moved or resized:
    /// `[x, y, breite, höhe]` in points, as the system counts them.
    ///
    /// A program somebody opens every day should open the way they left it.
    /// Unset — or pointing at a screen that is no longer there — means, which
    /// is what it did before.
    pub window: Option<[f32; 4]>,
    /// Where the typefaces that come with the program
    /// are. Unset means "look beside the program", which is where the build
    /// puts them.
    ///
    /// The older keys that chose a face out of it (`font_family`, `font_mono`,
    /// `font_regular`, `font_bold`) are gone; an older settings file still
    /// holding them is read as if they were not there. See `typeface`.
    pub font_dir: Option<String>,
    /// Which family in that folder the window is set in, by its key
    /// („asap", „muto"). Empty: never chosen — Muto where it came along, else Asap; see
    /// `gui::theme::PREFERRED`.
    pub typeface: String,
    /// How wide the typeface is set, on its width axis, where it has one:
    /// Asap's runs from 75 (narrow) to 125 (wide), 100 is as drawn. A face
    /// without the axis ignores it.
    pub font_width: f32,
    /// Whether the menu offers this machine's own typefaces too, after the ones
    /// in `fonts`: for trying faces out, not for keeping. The row that sets it
    /// shows with ⌥ held, or while it is on.
    pub system_fonts: bool,
    /// How heavy the regular is set, on a face with a weight axis: 400 is the
    /// face's regular, and the bold stands two hundred above wherever this is.
    /// A face without the axis ignores it.
    pub font_weight: f32,
    /// How the letters are fitted to the screen's pixels: „voll" (egui's own,
    /// as until then), „leicht" (only up and down), „aus" (the outline as
    /// drawn, as a pdf shows it). See `gui:theme:Hinting`.
    pub font_hinting: String,
    /// Which of each face's alternate forms are drawn, by the face's key and
    /// the feature's tag with the letter: `asap = ["ss01:g"]` is Asap's
    /// single-storey g, `muto = ["ss01:a"]` Muto's open a. A face not named
    /// here draws its default forms. See `gui::theme::style_sets`.
    pub font_sets: std::collections::BTreeMap<String, Vec<String>>,
    /// Path to the face used for characters the chosen one has no glyph for.
    /// Unset means "find one on this machine", which is what almost everyone
    /// wants; set it to name a different one.
    pub font_fallback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
    /// Default volume (0-100).
    pub volume: u8,
    /// Whether to enable gapless playback.
    pub gapless: bool,
    /// Levelling: „aus", „pro titel" or „pro album" — see
    /// `player::leveling`.
    pub leveling: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            music_dirs: vec![],
            watch_dirs: true,
            add_move: false,
            add_to_music: false,
            ipod_convert: true,
            ipod_format: "aac 256".into(),
            ipod_on_connect: "nichts".into(),
            ipod_mount: true,
            ipod_take_plays: true,
            ipod_podcasts: false,
            database_path: None,
            ui: UiConfig::default(),
            player: PlayerConfig::default(),
            set_aside: None,
            lastfm: LastfmConfig {
                now_playing: true,
                ..Default::default()
            },
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            ground: "公園".into(),
            track_columns: vec!["nr".into(), "jahr".into(), "genre".into(), "bitrate".into(), "länge".into()],
            artist_counts: false,
            artist_letters: true,
            artist_letter_bar: false,
            artist_total: false,
            albums_in_tracks: true,
            compilations_whole: true,
            library_line: true,
            show_compilations: true,
            radio: false,
            ipod: true,
            podcasts: false,
            music: false,
            capitals: false,
            touchbar: true,
            touchbar_view: "suchfeld".into(),
            touchbar_whole: false,
            pause_unplugged: true,
            podcast_back: 10,
            podcast_ahead: 10,
            podcast_keep: "gehört".into(),
            podcast_show_archived: true,
            podcast_check: "30".into(),
            podcast_queue: true,
            radio_info: true,
            podcast_order: "zuletzt".into(),
            podcast_letters: true,
            podcast_archive_forgets: true,
            radio_scrobble: true,
            radio_love: false,
            eq_on: false,
            eq_own_1: String::new(),
            eq_own_2: String::new(),
            eq_slot: 0,
            scroll_speed: "normal".into(),
            eq_gains: String::new(),
            radio_logos: true,
            radio_song_covers: true,
            radio_found: "rechts".to_string(),
            radio_order: "eigene".to_string(),
            radio_letters: true,
            show_recent: true,
            show_all_artists: true,
            start_on_recent: true,
            show_lists: false,
            all_columns: false,
            playing_at: "kompakt".to_string(),
            saver_after: 0,
            saver_dots: 2,
            saver_glow: 1,
            saver_strength: 1,
            saver_size: 2,
            saver_silent: false,
            saver_dither_method: "geordnet".into(),
            saver_colours: 0,
            saver_picture: "vektorskop".into(),
            menubar: "aus".into(),
            custom_colours: String::new(),
            menubar_cover: "daneben".into(),
            menubar_cover_screened: true,
            menubar_ink: "system".into(),
            language: String::new(),
            zoom: 1.0,
            brightness: "system".into(),
            merged_number: true,
            merged_length: true,
            merged_bitrate: true,
            numbers_right: false,
            small_length: false,
            device_mark: "wort".into(),
            notify: false,
            update_on_start: false,
            heading_count: true,
            heading_plain: false,
            modes_in_queue: false,
            info_picture: true,
            info_picture_wide: false,
            info_facts: true,
            info_wikipedia: true,
            info_library: true,
            info_ties: true,
            info_similar: true,
            lyrics_size: "normal".into(),
            lyrics_follow: true,
            log_hidden: Vec::new(),
            others_shown: 0,
            others_tree: false,
            artist_order: String::new(),
            album_order: String::new(),
            track_order: String::new(),
            field_spectrum: false,
            field_picture: "vektorskop".into(),
            book_at: 0,
            frame_cap: if SMALL_MACHINE { 30 } else { 0 },
            scope_rate: if SMALL_MACHINE { 12 } else { 20 },
            field_rate: 4,
            meter_rate: 12,
            cover_dither: true,
            picture_outline: "aus".into(),
            window_buttons: "einfarbig".into(),
            window_bar: "system".into(),
            keymap: "vim".into(),
            view_fade: true,
            columns_to_edge: true,
            foot_clock: "aus".into(),
            dither_method: "blaues rauschen".into(),
            dither_fineness: 0.6,
            dither_tones: 4,
            heading_cover_size: HEADING_COVER_DEFAULT,
            heading_covers: true,
            font_dir: None,
            typeface: String::new(),
            font_width: 85.0,
            system_fonts: false,
            font_weight: 400.0,
            font_hinting: "voll".into(),
            font_sets: [("asap", "ss01:g"), ("muto", "ss01:a"), ("mutomono", "ss01:a")]
                .into_iter()
                .map(|(face, tag)| (face.to_string(), vec![tag.to_string()]))
                .collect(),
            font_fallback: None,
            artists_share: None,
            albums_share: None,
            queue_share: None,
            side_share: None,
            radio_stations_share: None,
            radio_side_share: None,
            radio_page_share: None,
            podcast_shares: Vec::new(),
            merged_even: true,
            show_queue: true,
            window: None,
        }
    }
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            volume: 70,
            gapless: true,
            leveling: "aus".to_string(),
        }
    }
}

impl Config {
    /// Returns the platform-appropriate config directory.
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kessler")
    }

    /// Returns the platform-appropriate data directory.
    pub fn data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kessler")
    }

    /// Returns the path to the config file.
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// Where extracted album covers are kept.
    ///
    /// Beside the database rather than in a fixed place, so that pointing
    /// `database_path` at another library does not have the two of them
    /// sharing one pile of covers.
    /// Where what is known about artists is kept: one file each, and a
    /// picture — see `library::artistinfo`.
    /// Where the words LRCLIB gave are kept, beside the artists.
    pub fn lyrics_dir(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("lyrics"))
            .unwrap_or_else(|| Self::data_dir().join("lyrics"))
    }

    pub fn artists_dir(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("artists"))
            .unwrap_or_else(|| Self::data_dir().join("artists"))
    }

    pub fn artwork_dir(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("artwork"))
            .unwrap_or_else(|| Self::data_dir().join("artwork"))
    }

    /// Where the playing queue waits between one run and the next.
    ///
    /// Beside the database rather than inside the settings file: it is not a
    /// setting, it changes with every track, and a settings file rewritten
    /// that often is one waiting to be found half-written.
    pub fn queue_path(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("queue.txt"))
            .unwrap_or_else(|| Self::data_dir().join("queue.txt"))
    }

    /// What has been taken from each iPod's „Play Counts" already — see
    /// `ipod::playcounts::Taken`.
    pub fn ipod_plays_path(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("ipod-plays.toml"))
            .unwrap_or_else(|| Self::data_dir().join("ipod-plays.toml"))
    }

    /// Where scrobbles wait when they could not be sent.
    pub fn scrobble_queue_path(&self) -> PathBuf {
        self.db_path()
            .parent()
            .map(|d| d.join("scrobbles.tsv"))
            .unwrap_or_else(|| Self::data_dir().join("scrobbles.tsv"))
    }

    /// Returns the resolved database path.
    pub fn db_path(&self) -> PathBuf {
        self.database_path
            .clone()
            .unwrap_or_else(|| Self::data_dir().join("library.db"))
    }

    /// Load configuration from disk, creating defaults if not found.
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path).map_err(|e| {
                KesslerError::Config(format!("Failed to read config at {}: {}", config_path.display(), e))
            })?;
            let mut config: Config = match toml::from_str(&contents) {
                Ok(config) => config,
                Err(e) => return Ok(Self::set_aside(&config_path, &e.to_string())),
            };
            if !said_language(&contents) {
                config.ui.language = "de".to_string();
            }
            Ok(config)
        } else {
            let config = Config::default();
            if let Err(e) = config.save() {
                tracing::warn!("Could not save default config: {}", e);
            }
            Ok(config)
        }
    }

    /// A config file that cannot be read — a typo made by hand, half a file —
    /// put aside as `config.toml.kaputt` and the program started on the
    /// defaults, written out at the next change like any others. Refusing to
    /// start was the other answer, and on a Mac the application then simply did
    /// not open, with nothing to say why. The file is kept, not overwritten, so
    /// nothing in it is lost; one put aside before is replaced.
    fn set_aside(path: &std::path::Path, why: &str) -> Self {
        let aside = path.with_extension("toml.kaputt");
        eprintln!("kessler: config unreadable, set aside as {}: {why}", aside.display());
        tracing::warn!("config unreadable, set aside as {}: {why}", aside.display());
        let mut config = Config::default();
        if std::fs::rename(path, &aside).is_ok() {
            config.set_aside = Some(aside);
        }
        config
    }

    /// Load configuration from an explicit path (used by `--config`).
    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            KesslerError::Config(format!("Failed to read config at {}: {}", path.display(), e))
        })?;
        let config: Config = toml::from_str(&contents)
            .map_err(|e| KesslerError::Config(format!("Failed to parse config: {}", e)))?;
        Ok(config)
    }

    /// Save current configuration to disk.
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path();
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)
            .map_err(|e| KesslerError::Config(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(&config_path, contents)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

/// Whether a config file says anything about the language at all.
fn said_language(contents: &str) -> bool {
    contents
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("ui").and_then(|ui| ui.get("language")).cloned())
        .is_some()
}

/// The smallest, the usual and the largest cover on a record's heading. Linux
/// on ARM: a Raspberry Pi, most likely, with a few slow cores where a Mac has
/// many fast ones. It starts with the window drawing less often — thirty frames
/// a second for the hand, twelve for the scope — and the menu has the rest.
/// Only the defaults: a value in the file stays.
pub const SMALL_MACHINE: bool = cfg!(all(target_os = "linux", target_arch = "aarch64"));

pub const HEADING_COVER_LEAST: f32 = 32.0;
pub const HEADING_COVER_DEFAULT: f32 = 80.0;
pub const HEADING_COVER_MOST: f32 = 256.0;

/// How many tracks „zuletzt" shows at first, and fetches more of each time it
/// is scrolled to its end; also the length of the iPod's „zuletzt hinzugefügt".
/// Until then `[ui] recent_count`, set under „länge" (40, 80, 160 or 400),
/// which old configs still carry and nothing reads.
pub const RECENT_SHOWN: usize = 80;

impl UiConfig {
    /// The alternate forms chosen for the face `key` — see `font_sets`.
    pub fn sets_of(&self, key: &str) -> Vec<String> {
        self.font_sets.get(key).cloned().unwrap_or_default()
    }

    /// The heading cover's size, kept inside what the layout was drawn for —
    /// a hand-edited file can say anything.
    pub fn heading_cover(&self) -> f32 {
        let size = self.heading_cover_size;
        if size.is_finite() {
            size.clamp(HEADING_COVER_LEAST, HEADING_COVER_MOST)
        } else {
            HEADING_COVER_DEFAULT
        }
    }

    /// How fine the pictures' dots are, 0 … 1.
    pub fn picture_fineness(&self) -> f32 {
        if self.dither_fineness.is_finite() {
            self.dither_fineness.clamp(0.0, 1.0)
        } else {
            0.5
        }
    }
}

impl UiConfig {
    /// Every column as wide as the others again — the library's, the radio's
    /// and the podcasts', as the window opens. A drag holds until kessler is
    /// quit.
    pub fn even_columns_again(&mut self) {
        self.merged_even = true;
        self.artists_share = None;
        self.albums_share = None;
        self.queue_share = None;
        self.side_share = None;
        self.radio_stations_share = None;
        self.radio_side_share = None;
        self.radio_page_share = None;
        self.podcast_shares.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a first start looks like.
    #[test]
    fn a_first_start_has_the_chosen_defaults() {
        let ui = UiConfig::default();
        assert_eq!(ui.ground, "公園");
        assert_eq!(ui.brightness, "system");
        assert!(ui.show_all_artists && ui.show_recent && ui.start_on_recent && ui.show_compilations);
        assert!(!ui.artist_counts);
        assert!(ui.albums_in_tracks && ui.heading_covers && ui.heading_count && ui.compilations_whole);
        assert_eq!(ui.heading_cover(), 80.0);
        assert_eq!(ui.track_columns, ["nr", "jahr", "genre", "bitrate", "länge"]);
        assert!(!ui.numbers_right && ui.merged_number && ui.merged_length && ui.merged_bitrate);
        assert_eq!(ui.saver_after, 0, "no screen saver");
        assert!(!ui.field_spectrum, "the scope costs, so it waits to be asked for");
        assert_eq!(ui.language, "", "the system's");
        assert_eq!(ui.brightness, "system");
        assert_eq!((ui.typeface.as_str(), ui.font_width, ui.font_weight), ("", 85.0, 400.0));
        assert!(ui.cover_dither);
        assert_eq!((ui.dither_method.as_str(), ui.dither_tones, ui.dither_fineness), ("blaues rauschen", 4, 0.6));
        assert_eq!(ui.playing_at, "kompakt");
        assert!(ui.library_line && !ui.modes_in_queue);
    }

    /// A first start has nothing switched on; a file from before the music
    /// could be switched off keeps it on.
    #[test]
    fn the_music_is_off_at_first_and_on_in_old_files() {
        assert!(!Config::default().ui.music);
        let old: Config = toml::from_str("[ui]\nradio = true\n").unwrap();
        assert!(old.ui.music);
        let said: Config = toml::from_str("[ui]\nmusic = false\n").unwrap();
        assert!(!said.ui.music);
    }

    /// A key the settings no longer have is passed over, not refused: an
    /// old file still loads, with everything else it says.
    #[test]
    fn a_key_gone_from_the_settings_is_passed_over() {
        let old: Config = toml::from_str("[ui]\nheader_playing = true\nradio = true\n").unwrap();
        assert!(old.ui.radio);
    }

    /// A file that is not TOML is moved aside whole, and what comes back are
    /// the defaults, saying where it went.
    #[test]
    fn an_unreadable_config_is_set_aside() {
        let dir = std::env::temp_dir().join(format!("kessler-aside-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "music_dirs = [\n[ui]\n").unwrap();
        let config = Config::set_aside(&path, "invalid array");
        let aside = dir.join("config.toml.kaputt");
        assert_eq!(config.set_aside.as_deref(), Some(aside.as_path()));
        assert!(!path.exists());
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "music_dirs = [\n[ui]\n");
        assert!(config.music_dirs.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A config from before the choice existed says nothing about it; one
    /// written since says it, even when it says „the system's".
    #[test]
    fn a_config_says_its_language_or_nothing() {
        assert!(!said_language("[ui]\nground = \"nord\"\n"));
        assert!(!said_language("language = \"en\"\n"));
        assert!(said_language("[ui]\nlanguage = \"\"\n"));
        assert!(said_language("[ui]\nlanguage = \"en\"\n"));
        assert!(!said_language("not toml at all ["));
    }

    /// The headings' cover keeps to the sizes it was drawn for.
    #[test]
    fn a_heading_cover_keeps_to_its_sizes() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.heading_cover(), HEADING_COVER_DEFAULT);
        ui.heading_cover_size = 900.0;
        assert_eq!(ui.heading_cover(), HEADING_COVER_MOST);
        ui.heading_cover_size = f32::NAN;
        assert_eq!(ui.heading_cover(), HEADING_COVER_DEFAULT);
    }

    /// A settings file with no picture settings of its own keeps the base.
    #[test]
    fn an_older_settings_file_keeps_its_pictures() {
        let ui: UiConfig = toml::from_str("cover_dither = true\ndither_fineness = 0.3\n").unwrap();
        assert!(ui.cover_dither);
        assert!((ui.picture_fineness() - 0.3).abs() < 1e-6);
    }

}
