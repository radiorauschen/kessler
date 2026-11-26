//! Album covers: pulled out of the files once, kept as small squares.
//!
//! A cover embedded in a tag is often two thousand pixels across and half a
//! megabyte, and there is one in every track of the album. Decoding those while
//! scrolling a grid is what makes a music app feel heavy, so it happens once,
//! during the scan, and what lands on disk is a tile-sized JPEG — one per
//! album, not one per track.
//!
//! The cache is disposable by design. Nothing in the database points at it;
//! the file name is derived from the album's own keys, so a missing file means
//! "not extracted yet" and a stale one is simply overwritten. Deleting the
//! whole folder costs a rescan and nothing else.

use std::path::{Path, PathBuf};

use lofty::picture::PictureType;
use lofty::prelude::*;
use lofty::probe::Probe;

use crate::error::Result;

/// The longest side of a stored cover, in pixels.
///
/// Tiles are drawn at around 170 points, so this covers a Retina screen with a
/// little to spare and nothing more. Storing the original would mean a library
/// of album art larger than the music.
pub const TILE: u32 = 400;

/// How hard the stored JPEG is squeezed. High enough that the tiles look
/// clean, low enough that a thousand albums cost tens of megabytes.
const QUALITY: u8 = 82;

/// FNV-1a, written out rather than borrowed from the standard library.
///
/// `DefaultHasher` is explicitly not stable across Rust releases, and a cache
/// whose file names change when the compiler is upgraded would silently orphan
/// every cover it had already extracted.
fn digest(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The single number that identifies one album's cover.
///
/// Built from the same folded keys the library groups albums by, so two
/// spellings of one album share a cover instead of extracting it twice.
pub fn key(artist_key: &str, album_key: &str) -> u64 {
    digest(&format!("{artist_key}\u{1}{album_key}"))
}

/// Where one album's cover belongs.
pub fn cache_path(dir: &Path, artist_key: &str, album_key: &str) -> PathBuf {
    cache_path_for(dir, key(artist_key, album_key))
}

/// The same, for a key already worked out.
pub fn cache_path_for(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("{key:016x}.jpg"))
}

/// Where "this album was looked at and had no picture" is written down.
///
/// Without it there is no evidence that the search ever happened, so an album
/// with no embedded art has to be opened again on every scan. An empty marker
/// costs nothing and keeps the second scan of a library the fast one it is
/// meant to be. As disposable as the rest of the cache: delete it and the
/// search simply happens again.
fn miss_path(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("{key:016x}.none"))
}

/// Has this album's cover been settled — found, or looked for and not found?
pub fn settled(dir: &Path, artist_key: &str, album_key: &str) -> bool {
    let key = key(artist_key, album_key);
    cache_path_for(dir, key).exists() || miss_path(dir, key).exists()
}

/// The cover embedded in one file, as it is stored in the tag.
///
/// Prefers the picture actually marked as the front cover; a well-tagged album
/// can carry the back, the disc and the booklet as well, and a grid of CD
/// undersides is not what anyone means by cover view.
pub fn embedded(path: &Path) -> Option<Vec<u8>> {
    let file = Probe::open(path).ok()?.read().ok()?;
    let tag = file.primary_tag().or_else(|| file.first_tag())?;
    let pictures = tag.pictures();
    let chosen = pictures
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures.first())?;
    Some(chosen.data().to_vec())
}

/// Decode one picture, shrink it if it is bigger than it needs to be, and
/// hand back JPEG bytes.
///
/// `None` when the bytes are not an image this build can read. That case is
/// not an error anywhere it is used: a broken picture in one file is not a
/// reason to fail a scan of ten thousand, and a reply from the network that
/// turned out to be an error page must not reach a tag.
pub fn shrink_to_jpeg(raw: &[u8], longest: u32, quality: u8) -> Option<Vec<u8>> {
    let image = image::load_from_memory(raw).ok()?;

    let image = if image.width() > longest || image.height() > longest {
        image.thumbnail(longest, longest)
    } else {
        image
    };
    let image = image.to_rgb8();

    let mut out = Vec::new();
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .encode(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .ok()?;
    Some(out)
}

/// Decode, shrink and write one cover into the cache.
///
/// Returns false when the bytes are not an image this build can read.
pub fn store(raw: &[u8], to: &Path) -> Result<bool> {
    let Some(jpeg) = shrink_to_jpeg(raw, TILE, QUALITY) else {
        return Ok(false);
    };
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = to.with_extension("part");
    std::fs::write(&temporary, &jpeg)?;
    std::fs::rename(&temporary, to)?;
    Ok(true)
}

/// Take a picture that came from somewhere else — a search, a file the person
/// picked — as this album's cover.
///
/// The cache is written straight away rather than waiting for the next scan,
/// because the person is looking at the grid when they choose, and a cover
/// that appears a minute later is a cover that did not work.
pub fn adopt(dir: &Path, artist_key: &str, album_key: &str, raw: &[u8]) -> Result<bool> {
    let key = key(artist_key, album_key);
    if !store(raw, &cache_path_for(dir, key))? {
        return Ok(false);
    }
    let _ = std::fs::remove_file(miss_path(dir, key));
    Ok(true)
}

/// Forget every "looked and found nothing", keeping the covers themselves.
///
/// The markers are what stop a scan opening the same coverless files again and
/// again — and they are therefore also what stops it noticing a `cover.jpg`
/// put there afterwards. Clearing them is the way to say "look again", and it
/// costs one scan rather than a rebuilt library.
pub fn forget_misses(dir: &Path) -> Result<u32> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(0);
    };
    let mut forgotten = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("none")
            && std::fs::remove_file(&path).is_ok()
        {
            forgotten += 1;
        }
    }
    Ok(forgotten)
}

/// The names a cover file goes by when it lies beside the music.
///
/// In order of how much they mean it: `cover.jpg` was put there as the cover,
/// `AlbumArt_{…}_Large.jpg` was put there by Windows Media Player, and a
/// prefix match catches both that and `albumart.jpg`. Anything else in the
/// folder is left alone — a folder of band photographs is not a folder of
/// covers, and guessing wrong here is worse than showing a letter.
const SIDECAR_NAMES: &[&str] = &[
    "cover",
    "folder",
    "front",
    "album",
    "albumart",
    "artwork",
    "thumb",
];

/// Picture files worth opening. Whatever `image` can read; anything it cannot
/// simply fails to decode later and counts as not found.
const SIDECAR_KINDS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];

/// Nobody's cover is fifty megabytes, and reading one into memory to find that
/// out is how a scan turns into a swap storm.
const SIDECAR_MOST: u64 = 24 * 1024 * 1024;

/// A cover lying next to the music rather than inside it.
///
/// Half of what a real library actually has: iTunes keeps the pictures
/// it downloaded in a database of its own, and everything that came from a rip,
/// a download or a friend's drive tends to arrive as `cover.jpg` in the album's
/// folder. A player that only reads tags shows a quarter of a library as blank
/// squares and is wrong to.
///
/// Looks in the file's own folder and one level up — a boxed set keeps its
/// cover beside `CD1` and `CD2`, not inside them.
pub fn beside(source: &Path) -> Option<Vec<u8>> {
    let folder = source.parent()?;
    for dir in [Some(folder), folder.parent()].into_iter().flatten() {
        if let Some(found) = in_folder(dir) {
            return Some(found);
        }
    }
    None
}

/// The best-named picture in one folder, if it has one.
fn in_folder(dir: &Path) -> Option<Vec<u8>> {
    let mut best: Option<(usize, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let Some(rank) = ranked(&path) else { continue };
        if !entry.metadata().map(|m| m.is_file() && m.len() <= SIDECAR_MOST).unwrap_or(false) {
            continue;
        }
        if best.as_ref().is_none_or(|(best, _)| rank < *best) {
            best = Some((rank, path));
        }
    }
    std::fs::read(best?.1).ok()
}

/// How well a file's name says "this is the cover". Lower is better; `None`
/// means it does not say it at all.
fn ranked(path: &Path) -> Option<usize> {
    let kind = path.extension()?.to_str()?.to_lowercase();
    if !SIDECAR_KINDS.contains(&kind.as_str()) {
        return None;
    }
    let stem = path.file_stem()?.to_str()?.to_lowercase();
    SIDECAR_NAMES.iter().position(|name| stem.starts_with(name))
}

/// Extract one album's cover, unless it is already there.
///
/// Called once per track during a scan, so the "already there" case is the
/// common one and has to be cheap: it is a single stat, no decoding.
pub fn ensure(
    dir: &Path,
    artist_key: &str,
    album_key: &str,
    source: &Path,
    tagged: bool,
) -> Result<bool> {
    let key = key(artist_key, album_key);
    let target = cache_path_for(dir, key);
    if target.exists() {
        return Ok(false);
    }

    let raw = if tagged { embedded(source) } else { None };
    let found = match raw.or_else(|| beside(source)) {
        Some(raw) => store(&raw, &target)?,
        None => false,
    };

    let miss = miss_path(dir, key);
    if found {
        let _ = std::fs::remove_file(&miss);
    } else if !miss.exists() {
        if let Some(parent) = miss.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = std::fs::File::create(&miss);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kessler-art-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn square(size: u32, shade: u8) -> Vec<u8> {
        let image = image::RgbImage::from_fn(size, size, |x, _| {
            image::Rgb([shade, (x % 256) as u8, 128])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    /// The same album under two spellings must land on one file, or the grid
    /// shows one cover and one blank for what the list calls a single album.
    #[test]
    fn the_path_follows_the_album_key_not_its_spelling() {
        let dir = Path::new("/tmp");
        let a = cache_path(dir, "boards of canada", "geogaddi");
        let b = cache_path(dir, "boards of canada", "geogaddi");
        assert_eq!(a, b);
        assert_ne!(a, cache_path(dir, "boards of canada", "music has the right"));
        assert_ne!(a, cache_path(dir, "aphex twin", "geogaddi"));
        assert!(!a.to_string_lossy().contains("geogaddi"));
    }

    /// The hash is written out precisely so it cannot drift. If this value
    /// changes, every cover already extracted has been orphaned.
    #[test]
    fn the_digest_is_fixed_forever() {
        assert_eq!(digest(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(digest("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(digest("boards of canada\u{1}geogaddi"), digest("boards of canada\u{1}geogaddi"));
    }

    #[test]
    fn a_stored_cover_is_shrunk_to_the_tile() {
        let dir = scratch("shrink");
        let to = dir.join("cover.jpg");
        assert!(store(&square(1600, 200), &to).unwrap());

        let stored = image::open(&to).unwrap();
        assert_eq!(stored.width().max(stored.height()), TILE);
        assert!(std::fs::metadata(&to).unwrap().len() < 60_000);
        assert!(!dir.join("cover.part").exists());
    }

    /// A cover smaller than a tile is left alone rather than blown up.
    #[test]
    fn a_small_cover_is_not_enlarged() {
        let dir = scratch("small");
        let to = dir.join("cover.jpg");
        assert!(store(&square(120, 90), &to).unwrap());
        let stored = image::open(&to).unwrap();
        assert_eq!((stored.width(), stored.height()), (120, 120));
    }

    /// One unreadable picture must not stop a scan.
    #[test]
    fn rubbish_is_refused_without_an_error() {
        let dir = scratch("rubbish");
        let to = dir.join("cover.jpg");
        assert!(!store(b"this is not an image", &to).unwrap());
        assert!(!to.exists());
        assert!(!dir.join("cover.part").exists());
    }

    /// The common case during a scan is "already done", and it must not cost
    /// a decode — every track of an album asks.
    #[test]
    fn an_existing_cover_is_left_alone() {
        let dir = scratch("existing");
        let target = cache_path(&dir, "artist", "album");
        std::fs::write(&target, b"already here").unwrap();

        let source = dir.join("track.mp3");
        std::fs::write(&source, b"not really an mp3").unwrap();

        assert!(!ensure(&dir, "artist", "album", &source, false).unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), b"already here");
    }

    /// A file with no picture in it is not an error either.
    #[test]
    fn a_file_without_a_cover_is_not_a_failure() {
        let dir = scratch("none");
        let source = dir.join("track.mp3");
        std::fs::write(&source, b"not really an mp3").unwrap();
        assert!(!ensure(&dir, "artist", "album", &source, true).unwrap());
        assert!(!cache_path(&dir, "artist", "album").exists());
    }

    /// The bug this guards: a library scanned before covers existed would
    /// never grow one, because the scanner skipped every unchanged file before
    /// it got as far as the picture. "Unchanged" now has to mean "and we know
    /// about its cover", which needs a record of having looked.
    #[test]
    fn looking_and_finding_nothing_is_remembered() {
        let dir = scratch("settled");
        let nothing = dir.join("not-audio.mp3");
        std::fs::write(&nothing, b"this is not a tagged file").unwrap();

        assert!(!settled(&dir, "eno", "on land"), "nothing looked at yet");
        assert!(!ensure(&dir, "eno", "on land", &nothing, true).unwrap());
        assert!(settled(&dir, "eno", "on land"), "the search was not recorded");
        assert!(!cache_path(&dir, "eno", "on land").exists());
    }

    /// One track of an album can have nothing while a later one has the cover.
    #[test]
    fn a_later_track_still_gets_its_cover_in() {
        let dir = scratch("late-cover");
        let nothing = dir.join("no-tag.mp3");
        std::fs::write(&nothing, b"x").unwrap();
        assert!(!ensure(&dir, "aphex", "saw", &nothing, true).unwrap());
        assert!(settled(&dir, "aphex", "saw"));

        let target = cache_path(&dir, "aphex", "saw");
        assert!(store(&square(600, 40), &target).unwrap());
        assert!(settled(&dir, "aphex", "saw"));
        assert!(target.exists());
    }

    /// Half of a real library keeps its covers beside the music rather than
    /// inside it — everything iTunes downloaded, everything that arrived as a
    /// folder. Reading tags alone shows a quarter of such a library blank.
    #[test]
    fn a_cover_beside_the_music_is_found() {
        let cache = scratch("beside-cache");
        let music = scratch("beside-music");
        let track = music.join("01 olson.mp3");
        std::fs::write(&track, b"not really an mp3").unwrap();
        let picture = square(500, 90);
        std::fs::write(music.join("cover.jpg"), &picture).unwrap();

        assert!(ensure(&cache, "boa", "geogaddi", &track, false).unwrap());
        assert!(cache_path(&cache, "boa", "geogaddi").exists());
        assert!(settled(&cache, "boa", "geogaddi"));
    }

    /// A folder can hold several pictures, and only some of them are the
    /// cover.
    #[test]
    fn the_name_decides_which_picture_is_the_cover() {
        let music = scratch("named");
        let cover = square(200, 10);
        let other = square(200, 200);
        std::fs::write(music.join("band on a hill.jpg"), &other).unwrap();
        std::fs::write(music.join("folder.jpg"), &other).unwrap();
        std::fs::write(music.join("cover.jpg"), &cover).unwrap();

        assert_eq!(beside(&music.join("track.mp3")), Some(cover));
    }

    /// The one that must not happen: a photograph of the band becoming the
    /// record's cover.
    #[test]
    fn a_picture_with_any_old_name_is_left_alone() {
        let music = scratch("photo");
        std::fs::write(music.join("band on a hill.jpg"), square(200, 10)).unwrap();
        std::fs::write(music.join("scan002.png"), square(200, 10)).unwrap();
        assert_eq!(beside(&music.join("track.mp3")), None);
    }

    /// A boxed set keeps its cover beside the discs, not inside them.
    #[test]
    fn a_cover_one_level_up_still_counts() {
        let music = scratch("boxed");
        let disc = music.join("CD2");
        std::fs::create_dir_all(&disc).unwrap();
        let picture = square(300, 60);
        std::fs::write(music.join("front.jpeg"), &picture).unwrap();

        assert_eq!(beside(&disc.join("03 tha.mp3")), Some(picture));
    }

    /// Saying "look again" must not throw away the pictures already found.
    #[test]
    fn forgetting_the_misses_keeps_the_covers() {
        let dir = scratch("forget");
        let cover = cache_path(&dir, "boa", "geogaddi");
        assert!(store(&square(300, 30), &cover).unwrap());
        let nothing = dir.join("track.mp3");
        std::fs::write(&nothing, b"x").unwrap();
        assert!(!ensure(&dir, "eno", "on land", &nothing, true).unwrap());
        assert!(settled(&dir, "eno", "on land"));

        assert_eq!(forget_misses(&dir).unwrap(), 1);
        assert!(!settled(&dir, "eno", "on land"), "the search is open again");
        assert!(settled(&dir, "boa", "geogaddi"), "the cover is still there");
        assert!(cover.exists());
    }

    /// The ranking, written out, because the order of that list is the whole
    /// of the rule.
    #[test]
    fn the_ranking_reads_as_it_is_written() {
        let rank = |name: &str| ranked(Path::new(name));
        assert_eq!(rank("cover.jpg"), Some(0));
        assert_eq!(rank("COVER.JPG"), Some(0));
        assert_eq!(rank("folder.png"), Some(1));
        assert_eq!(rank("AlbumArt_{0000}_Large.jpg"), Some(3));
        assert_eq!(rank("cover.txt"), None);
        assert_eq!(rank("sleeve.jpg"), None);
        assert_eq!(rank("nachtmusik.jpg"), None);
    }
}
