use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use rodio::Source;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trim {
    pub skip: f64,
    pub keep: Option<f64>,
}

pub fn trim_for(path: &Path) -> Option<Trim> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if !matches!(ext.as_str(), "m4a" | "m4b" | "mp4" | "m4p") {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let moov = find_top(&mut file, b"moov")?;
    let trim = parse_moov(&moov)?;
    (trim.skip > 0.0 || trim.keep.is_some()).then_some(trim)
}

fn find_top(file: &mut File, wanted: &[u8; 4]) -> Option<Vec<u8>> {
    let length = file.metadata().ok()?.len();
    let mut at = 0u64;
    while at.saturating_add(8) <= length {
        file.seek(SeekFrom::Start(at)).ok()?;
        let mut head = [0u8; 16];
        file.read_exact(&mut head[..8]).ok()?;
        let mut size = u32::from_be_bytes(head[..4].try_into().ok()?) as u64;
        let kind: [u8; 4] = head[4..8].try_into().ok()?;
        let mut header = 8;
        if size == 1 {
            file.read_exact(&mut head[8..16]).ok()?;
            size = u64::from_be_bytes(head[8..16].try_into().ok()?);
            header = 16;
        } else if size == 0 {
            size = length - at;
        }
        if size < header {
            return None;
        }
        if &kind == wanted {
            let body = size - header;
            if body > 64 * 1024 * 1024 || at.checked_add(size).is_none_or(|end| end > length) {
                return None;
            }
            let mut data = vec![0u8; body as usize];
            file.read_exact(&mut data).ok()?;
            return Some(data);
        }
        at = at.checked_add(size)?;
    }
    None
}

fn children(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 8 <= data.len() {
        let mut size = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = data[at + 4..at + 8].try_into().unwrap();
        let mut header = 8;
        if size == 1 {
            if at + 16 > data.len() {
                break;
            }
            size = u64::from_be_bytes(data[at + 8..at + 16].try_into().unwrap()) as usize;
            header = 16;
        } else if size == 0 {
            size = data.len() - at;
        }
        let Some(end) = at.checked_add(size).filter(|&end| size >= header && end <= data.len()) else {
            break;
        };
        out.push((kind, &data[at + header..end]));
        at = end;
    }
    out
}

fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    children(data).into_iter().find(|(k, _)| k == kind).map(|(_, b)| b)
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn be64(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(data.get(at..at + 8)?.try_into().ok()?))
}

fn timescale(full_box: &[u8]) -> Option<u32> {
    match full_box.first()? {
        1 => be32(full_box, 20),
        _ => be32(full_box, 12),
    }
}

fn parse_moov(moov: &[u8]) -> Option<Trim> {
    if let Some(trim) = itunsmpb(moov) {
        return Some(trim);
    }
    let movie_scale = timescale(child(moov, b"mvhd")?)? as f64;
    for (kind, trak) in children(moov) {
        if &kind != b"trak" {
            continue;
        }
        let mdia = child(trak, b"mdia")?;
        let handler = child(mdia, b"hdlr")?;
        if handler.get(8..12) != Some(b"soun") {
            continue;
        }
        let media_scale = timescale(child(mdia, b"mdhd")?)? as f64;
        let elst = child(child(trak, b"edts")?, b"elst")?;
        let version = *elst.first()?;
        let count = be32(elst, 4)?;
        let width = if version == 1 { 20 } else { 12 };
        for index in 0..count as usize {
            let at = 8 + index * width;
            let (duration, media_time) = if version == 1 {
                (be64(elst, at)?, be64(elst, at + 8)? as i64)
            } else {
                (be32(elst, at)? as u64, be32(elst, at + 4)? as i32 as i64)
            };
            if media_time < 0 {
                continue;
            }
            if movie_scale <= 0.0 || media_scale <= 0.0 {
                return None;
            }
            let keep = (duration > 0).then(|| duration as f64 / movie_scale);
            return Some(Trim {
                skip: media_time as f64 / media_scale,
                keep,
            });
        }
    }
    None
}

fn itunsmpb(moov: &[u8]) -> Option<Trim> {
    let meta = child(child(moov, b"udta")?, b"meta")?;
    let ilst = child(meta.get(4..)?, b"ilst")?;
    for (kind, item) in children(ilst) {
        if &kind != b"----" {
            continue;
        }
        let parts = children(item);
        let named = parts
            .iter()
            .any(|(k, b)| k == b"name" && b.get(4..) == Some(b"iTunSMPB"));
        if !named {
            continue;
        }
        let data = parts.iter().find(|(k, _)| k == b"data")?.1;
        let said = std::str::from_utf8(data.get(8..)?).ok()?;
        let fields: Vec<u64> = said
            .split_whitespace()
            .filter_map(|f| u64::from_str_radix(f, 16).ok())
            .collect();
        let (delay, _padding, length) = (*fields.get(1)?, *fields.get(2)?, *fields.get(3)?);
        let rate = sample_rate(moov)? as f64;
        return Some(Trim {
            skip: delay as f64 / rate,
            keep: (length > 0).then(|| length as f64 / rate),
        });
    }
    None
}

fn sample_rate(moov: &[u8]) -> Option<u32> {
    for (kind, trak) in children(moov) {
        if &kind != b"trak" {
            continue;
        }
        let mdia = child(trak, b"mdia")?;
        if child(mdia, b"hdlr")?.get(8..12) == Some(b"soun") {
            return timescale(child(mdia, b"mdhd")?);
        }
    }
    None
}

pub struct Trimmed<S> {
    inner: S,
    skip: usize,
    left: Option<usize>,
    trim: Option<Trim>,
    channels: usize,
    rate: u32,
}

impl<S: Source> Trimmed<S> {
    pub fn new(inner: S, trim: Option<Trim>) -> Self {
        let channels = usize::from(inner.channels().get());
        let rate = inner.sample_rate().get();
        let trim = trim.filter(|t| match (inner.total_duration(), t.keep) {
            (Some(total), Some(keep)) => total.as_secs_f64() > keep + 0.001,
            _ => t.skip > 0.0,
        });
        let mut trimmed = Self {
            inner,
            skip: 0,
            left: None,
            trim,
            channels,
            rate,
        };
        trimmed.reset(Duration::ZERO);
        trimmed
    }

    fn samples(&self, seconds: f64) -> usize {
        ((seconds * self.rate as f64).round().max(0.0) as usize) * self.channels
    }

    fn reset(&mut self, pos: Duration) {
        let Some(trim) = self.trim else {
            return;
        };
        self.skip = if pos.is_zero() { self.samples(trim.skip) } else { 0 };
        self.left = trim
            .keep
            .map(|keep| self.samples((keep - pos.as_secs_f64()).max(0.0)));
    }
}

impl<S: Source> Iterator for Trimmed<S> {
    type Item = rodio::Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while self.skip > 0 {
            self.inner.next()?;
            self.skip -= 1;
        }
        if let Some(left) = &mut self.left {
            if *left == 0 {
                return None;
            }
            *left -= 1;
        }
        self.inner.next()
    }
}

impl<S: Source> Source for Trimmed<S> {
    fn current_span_len(&self) -> Option<usize> {
        if self.trim.is_some() {
            None
        } else {
            self.inner.current_span_len()
        }
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        match self.trim {
            Some(Trim { keep: Some(keep), .. }) => Some(Duration::from_secs_f64(keep)),
            Some(Trim { skip, keep: None }) => self
                .inner
                .total_duration()
                .map(|d| d.saturating_sub(Duration::from_secs_f64(skip))),
            None => self.inner.total_duration(),
        }
    }

    fn try_seek(&mut self, pos: Duration) -> std::result::Result<(), rodio::source::SeekError> {
        let run_up = self
            .trim
            .map(|t| Duration::from_secs_f64(t.skip))
            .unwrap_or_default();
        self.inner.try_seek(pos + run_up)?;
        self.reset(pos);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_past_any_number_is_a_broken_file() {
        let mut huge = vec![0, 0, 0, 1];
        huge.extend_from_slice(b"free");
        huge.extend_from_slice(&u64::MAX.to_be_bytes());
        huge.extend_from_slice(&[0; 16]);
        assert!(children(&huge).is_empty());
        let path = std::env::temp_dir().join(format!("kessler-huge-{}.m4a", std::process::id()));
        std::fs::write(&path, &huge).unwrap();
        assert!(trim_for(&path).is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_edit_list_is_read() {
        let mut moov = Vec::new();
        let boxed = |kind: &[u8; 4], body: &[u8]| {
            let mut b = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            b.extend_from_slice(kind);
            b.extend_from_slice(body);
            b
        };
        let mut mvhd = vec![0u8; 12];
        mvhd.extend_from_slice(&1000u32.to_be_bytes());
        mvhd.extend_from_slice(&[0u8; 80]);
        let mut mdhd = vec![0u8; 12];
        mdhd.extend_from_slice(&44100u32.to_be_bytes());
        mdhd.extend_from_slice(&[0u8; 8]);
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(b"soun");
        hdlr.extend_from_slice(&[0u8; 12]);
        let mut elst = vec![0, 0, 0, 0, 0, 0, 0, 1];
        elst.extend_from_slice(&3000u32.to_be_bytes());
        elst.extend_from_slice(&1024u32.to_be_bytes());
        elst.extend_from_slice(&[0, 1, 0, 0]);
        let mdia = [boxed(b"mdhd", &mdhd), boxed(b"hdlr", &hdlr)].concat();
        let trak = [boxed(b"edts", &boxed(b"elst", &elst)), boxed(b"mdia", &mdia)].concat();
        moov.extend(boxed(b"mvhd", &mvhd));
        moov.extend(boxed(b"trak", &trak));

        let trim = parse_moov(&moov).expect("an edit list");
        assert!((trim.skip - 1024.0 / 44100.0).abs() < 1e-9);
        assert_eq!(trim.keep, Some(3.0));
    }

    #[test]
    fn itunes_note_is_read() {
        let boxed = |kind: &[u8; 4], body: &[u8]| {
            let mut b = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            b.extend_from_slice(kind);
            b.extend_from_slice(body);
            b
        };
        let mut mdhd = vec![0u8; 12];
        mdhd.extend_from_slice(&44100u32.to_be_bytes());
        mdhd.extend_from_slice(&[0u8; 8]);
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(b"soun");
        hdlr.extend_from_slice(&[0u8; 12]);
        let trak = boxed(
            b"trak",
            &boxed(b"mdia", &[boxed(b"mdhd", &mdhd), boxed(b"hdlr", &hdlr)].concat()),
        );
        let said = " 00000000 00000840 000001CC 0000000000204E74 00000000 00000000";
        let mut data = vec![0u8; 8];
        data.extend_from_slice(said.as_bytes());
        let mut name = vec![0u8; 4];
        name.extend_from_slice(b"iTunSMPB");
        let item = boxed(
            b"----",
            &[boxed(b"mean", b"\0\0\0\0com.apple.iTunes"), boxed(b"name", &name), boxed(b"data", &data)]
                .concat(),
        );
        let mut meta = vec![0u8; 4];
        meta.extend(boxed(b"ilst", &item));
        let udta = boxed(b"udta", &boxed(b"meta", &meta));
        let moov = [trak, udta].concat();

        let trim = parse_moov(&moov).expect("the note");
        assert!((trim.skip - 0x840 as f64 / 44100.0).abs() < 1e-9);
        assert!((trim.keep.unwrap() - 0x204E74 as f64 / 44100.0).abs() < 1e-9);
    }

    #[test]
    fn other_formats_are_not_touched() {
        assert_eq!(trim_for(Path::new("/nowhere/a.mp3")), None);
        assert_eq!(trim_for(Path::new("/nowhere/a.flac")), None);
        assert_eq!(trim_for(Path::new("/nowhere/a.m4a")), None);
    }
}
