use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rodio::decoder::DecoderBuilder;
use rodio::Decoder;

const WAIT_AT_MOST: Duration = Duration::from_secs(20);

const LOOK_EVERY: Duration = Duration::from_millis(25);

pub struct Growing {
    file: File,
    done: Arc<AtomicBool>,
}

impl Read for Growing {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let since = Instant::now();
        loop {
            let n = self.file.read(buf)?;
            if n > 0 || buf.is_empty() {
                return Ok(n);
            }
            if self.done.load(Ordering::Acquire) {
                return self.file.read(buf);
            }
            if since.elapsed() >= WAIT_AT_MOST {
                return Ok(0);
            }
            std::thread::sleep(LOOK_EVERY);
        }
    }
}

impl Seek for Growing {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(to)
    }
}

pub fn open(part: &Path, done: Arc<AtomicBool>, bytes: u64, kind: &str) -> Result<Decoder<BufReader<Growing>>, String> {
    let file = File::open(part).map_err(|e| e.to_string())?;
    let growing = Growing { file, done };
    let kind = kind.to_string();
    let opened = super::audio::quietly(move || {
        let builder = DecoderBuilder::new().with_data(BufReader::new(growing)).with_seekable(true).with_hint(&kind);
        let builder = if bytes > 0 { builder.with_byte_len(bytes) } else { builder };
        builder.build().map_err(|e| e.to_string())
    });
    opened.unwrap_or_else(|_| Err("the decoder gave up on this file".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_read_waits_for_what_is_still_coming() {
        let dir = std::env::temp_dir().join(format!("kessler-growing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let part = dir.join("folge.part");
        let mut writer = File::create(&part).unwrap();
        writer.write_all(b"anfang").unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let mut growing = Growing { file: File::open(&part).unwrap(), done: Arc::clone(&done) };
        let later = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            writer.write_all(b" und ende").unwrap();
            writer.flush().unwrap();
            done.store(true, Ordering::Release);
        });
        let mut all = String::new();
        growing.read_to_string(&mut all).unwrap();
        later.join().unwrap();
        assert_eq!(all, "anfang und ende");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
