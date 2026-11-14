use unicode_normalization::UnicodeNormalization;

static TABLE: &[u8] = include_bytes!("misaki/misaki.bin");

const RECORD: usize = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Glyph {
    width: usize,
    rows: [u8; 8],
}

fn glyph(c: char) -> Option<Glyph> {
    let code = c as u32;
    let count = TABLE.len() / RECORD;
    let at = |i: usize| {
        let record = &TABLE[i * RECORD..(i + 1) * RECORD];
        u32::from_le_bytes([record[0], record[1], record[2], record[3]])
    };
    let (mut low, mut high) = (0, count);
    while low < high {
        let middle = (low + high) / 2;
        match at(middle).cmp(&code) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
            std::cmp::Ordering::Equal => {
                let record = &TABLE[middle * RECORD..(middle + 1) * RECORD];
                let mut rows = [0; 8];
                rows.copy_from_slice(&record[5..]);
                return Some(Glyph { width: record[4] as usize, rows });
            }
        }
    }
    None
}

const DRAWN: [(char, [u8; 8]); 4] = [
    ('Ä', [0b1010_0000, 0b0100_0000, 0b1010_0000, 0b1110_0000, 0b1010_0000, 0b1010_0000, 0, 0]),
    ('Ö', [0b1010_0000, 0b0100_0000, 0b1010_0000, 0b1010_0000, 0b1010_0000, 0b0100_0000, 0, 0]),
    ('Ü', [0b1010_0000, 0, 0b1010_0000, 0b1010_0000, 0b1010_0000, 0b1110_0000, 0, 0]),
    ('„', [0, 0, 0, 0, 0, 0b0101_0000, 0b1010_0000, 0]),
];

fn spelled(c: char, out: &mut Vec<Glyph>) {
    if let Some(found) = glyph(c) {
        out.push(found);
        return;
    }
    if let Some((_, rows)) = DRAWN.iter().find(|(drawn, _)| *drawn == c) {
        out.push(Glyph { width: 4, rows: *rows });
        return;
    }
    match c {
        'ß' => return spelled_str("ss", out),
        'ẞ' => return spelled_str("SS", out),
        'æ' => return spelled_str("ae", out),
        'œ' => return spelled_str("oe", out),
        'ø' => return spelled('o', out),
        '–' | '—' => return spelled('-', out),
        '‘' | '’' => return spelled('\'', out),
        '“' | '”' => return spelled('"', out),
        c if c.is_whitespace() => return spelled(' ', out),
        _ => {}
    }
    let mut parts = std::iter::once(c).nfd();
    if let Some(base) = parts.next().and_then(glyph) {
        let mut letter = base;
        for mark in parts {
            if letter.rows[0] != 0 || letter.rows[1] != 0 {
                break;
            }
            letter.rows[0] |= if mark == '\u{308}' { 0b1010_0000 } else { 0b0100_0000 };
        }
        out.push(letter);
        return;
    }
    if let Some(unknown) = glyph('?') {
        out.push(unknown);
    }
}

fn spelled_str(text: &str, out: &mut Vec<Glyph>) {
    for c in text.chars() {
        spelled(c, out);
    }
}

pub fn columns(text: &str) -> Vec<u8> {
    let mut glyphs = Vec::new();
    spelled_str(text, &mut glyphs);
    let mut out = Vec::with_capacity(glyphs.iter().map(|g| g.width).sum());
    for g in glyphs {
        for x in 0..g.width {
            out.push((0..8).fold(0u8, |byte, row| {
                byte | (u8::from(g.rows[row] & (0x80 >> x) != 0) << row)
            }));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_whole_and_in_order() {
        assert_eq!(TABLE.len() % RECORD, 0);
        let codes: Vec<u32> = TABLE
            .chunks(RECORD)
            .map(|r| u32::from_le_bytes([r[0], r[1], r[2], r[3]]))
            .collect();
        assert!(codes.windows(2).all(|w| w[0] < w[1]));
        assert!(TABLE.chunks(RECORD).all(|r| r[4] == 4 || r[4] == 8));
    }

    #[test]
    fn latin_is_half_as_wide_as_kanji() {
        assert_eq!(columns("a").len(), 4);
        assert_eq!(columns("夜").len(), 8);
        assert_eq!(columns("ケスラー").len(), 32);
        assert_eq!(columns("a b").len(), 12);
    }

    #[test]
    fn an_umlaut_gets_its_dots_and_the_capitals_are_drawn() {
        let a = columns("a");
        let ä = columns("ä");
        assert_eq!(ä.len(), 4);
        assert_eq!(ä[0] & 1, 1);
        assert_eq!(ä[1] & 1, 0);
        assert_eq!(ä[2] & 1, 1);
        assert_eq!(ä[0] & !1, a[0]);
        assert_ne!(columns("Ä"), columns("A"));
        assert_eq!(columns("Ü").len(), 4);
        assert!(columns("ÄÖÜ„").iter().all(|column| column & 0b1000_0000 == 0));
        assert_eq!(columns("É"), columns("E"));
        assert_eq!(columns("ß"), columns("ss"));
    }

    #[test]
    fn what_is_not_there_is_a_question_mark() {
        assert_eq!(columns("\u{1F3B5}"), columns("?"));
    }
}
