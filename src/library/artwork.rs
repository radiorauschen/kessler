use std::path::{Path, PathBuf};

use lofty::picture::PictureType;
use lofty::prelude::*;
use lofty::probe::Probe;

use crate::error::Result;

pub const TILE: u32 = 400;

const QUALITY: u8 = 82;

fn digest(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn key(artist_key: &str, album_key: &str) -> u64 {
    digest(&format!("{artist_key}\u{1}{album_key}"))
}

pub fn cache_path(dir: &Path, artist_key: &str, album_key: &str) -> PathBuf {
    cache_path_for(dir, key(artist_key, album_key))
}

pub fn cache_path_for(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("{key:016x}.jpg"))
}

fn miss_path(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("{key:016x}.none"))
}

pub fn settled(dir: &Path, artist_key: &str, album_key: &str) -> bool {
    let key = key(artist_key, album_key);
    cache_path_for(dir, key).exists() || miss_path(dir, key).exists()
}

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

pub fn adopt(dir: &Path, artist_key: &str, album_key: &str, raw: &[u8]) -> Result<bool> {
    let key = key(artist_key, album_key);
    if !store(raw, &cache_path_for(dir, key))? {
        return Ok(false);
    }
    let _ = std::fs::remove_file(miss_path(dir, key));
    Ok(true)
}

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

const SIDECAR_NAMES: &[&str] = &[
    "cover",
    "folder",
    "front",
    "album",
    "albumart",
    "artwork",
    "thumb",
];

const SIDECAR_KINDS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];

const SIDECAR_MOST: u64 = 24 * 1024 * 1024;

pub fn beside(source: &Path) -> Option<Vec<u8>> {
    let folder = source.parent()?;
    for dir in [Some(folder), folder.parent()].into_iter().flatten() {
        if let Some(found) = in_folder(dir) {
            return Some(found);
        }
    }
    None
}

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

fn ranked(path: &Path) -> Option<usize> {
    let kind = path.extension()?.to_str()?.to_lowercase();
    if !SIDECAR_KINDS.contains(&kind.as_str()) {
        return None;
    }
    let stem = path.file_stem()?.to_str()?.to_lowercase();
    SIDECAR_NAMES.iter().position(|name| stem.starts_with(name))
}

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

    #[test]
    fn a_small_cover_is_not_enlarged() {
        let dir = scratch("small");
        let to = dir.join("cover.jpg");
        assert!(store(&square(120, 90), &to).unwrap());
        let stored = image::open(&to).unwrap();
        assert_eq!((stored.width(), stored.height()), (120, 120));
    }

    #[test]
    fn rubbish_is_refused_without_an_error() {
        let dir = scratch("rubbish");
        let to = dir.join("cover.jpg");
        assert!(!store(b"this is not an image", &to).unwrap());
        assert!(!to.exists());
        assert!(!dir.join("cover.part").exists());
    }

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

    #[test]
    fn a_file_without_a_cover_is_not_a_failure() {
        let dir = scratch("none");
        let source = dir.join("track.mp3");
        std::fs::write(&source, b"not really an mp3").unwrap();
        assert!(!ensure(&dir, "artist", "album", &source, true).unwrap());
        assert!(!cache_path(&dir, "artist", "album").exists());
    }

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

    #[test]
    fn a_picture_with_any_old_name_is_left_alone() {
        let music = scratch("photo");
        std::fs::write(music.join("band on a hill.jpg"), square(200, 10)).unwrap();
        std::fs::write(music.join("scan002.png"), square(200, 10)).unwrap();
        assert_eq!(beside(&music.join("track.mp3")), None);
    }

    #[test]
    fn a_cover_one_level_up_still_counts() {
        let music = scratch("boxed");
        let disc = music.join("CD2");
        std::fs::create_dir_all(&disc).unwrap();
        let picture = square(300, 60);
        std::fs::write(music.join("front.jpeg"), &picture).unwrap();

        assert_eq!(beside(&disc.join("03 tha.mp3")), Some(picture));
    }

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
