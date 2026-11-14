#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn parse(text: &str) -> Option<Json> {
        let mut reader = Reader {
            bytes: text.as_bytes(),
            at: 0,
        };
        reader.space();
        let value = reader.value()?;
        reader.space();
        if reader.at == reader.bytes.len() {
            Some(value)
        } else {
            None
        }
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_str())
    }

    pub fn list(&self, key: &str) -> &[Json] {
        self.get(key).and_then(|v| v.as_array()).unwrap_or(&[])
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn literal(&mut self, word: &str) -> Option<()> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Some(())
        } else {
            None
        }
    }

    fn value(&mut self) -> Option<Json> {
        match self.peek()? {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => self.string().map(Json::String),
            b't' => self.literal("true").map(|_| Json::Bool(true)),
            b'f' => self.literal("false").map(|_| Json::Bool(false)),
            b'n' => self.literal("null").map(|_| Json::Null),
            _ => self.number(),
        }
    }

    fn object(&mut self) -> Option<Json> {
        self.at += 1;
        let mut pairs = Vec::new();
        self.space();
        if self.peek()? == b'}' {
            self.at += 1;
            return Some(Json::Object(pairs));
        }
        loop {
            self.space();
            let key = self.string()?;
            self.space();
            if self.peek()? != b':' {
                return None;
            }
            self.at += 1;
            self.space();
            let value = self.value()?;
            pairs.push((key, value));
            self.space();
            match self.peek()? {
                b',' => self.at += 1,
                b'}' => {
                    self.at += 1;
                    return Some(Json::Object(pairs));
                }
                _ => return None,
            }
        }
    }

    fn array(&mut self) -> Option<Json> {
        self.at += 1;
        let mut items = Vec::new();
        self.space();
        if self.peek()? == b']' {
            self.at += 1;
            return Some(Json::Array(items));
        }
        loop {
            self.space();
            items.push(self.value()?);
            self.space();
            match self.peek()? {
                b',' => self.at += 1,
                b']' => {
                    self.at += 1;
                    return Some(Json::Array(items));
                }
                _ => return None,
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        if self.peek()? != b'"' {
            return None;
        }
        self.at += 1;
        let mut out = String::new();
        loop {
            let byte = self.peek()?;
            match byte {
                b'"' => {
                    self.at += 1;
                    return Some(out);
                }
                b'\\' => {
                    self.at += 1;
                    match self.peek()? {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            out.push(self.escaped_char()?);
                        }
                        _ => return None,
                    }
                    self.at += 1;
                }
                _ => {
                    let start = self.at;
                    while let Some(b) = self.peek() {
                        if b == b'"' || b == b'\\' {
                            break;
                        }
                        self.at += 1;
                    }
                    out.push_str(std::str::from_utf8(&self.bytes[start..self.at]).ok()?);
                }
            }
        }
    }

    fn escaped_char(&mut self) -> Option<char> {
        let first = self.hex4()?;
        if (0xD800..0xDC00).contains(&first) {
            if self.bytes.get(self.at + 1) != Some(&b'\\')
                || self.bytes.get(self.at + 2) != Some(&b'u')
            {
                return None;
            }
            self.at += 2;
            let second = self.hex4()?;
            if !(0xDC00..0xE000).contains(&second) {
                return None;
            }
            let combined =
                0x1_0000 + ((first as u32 - 0xD800) << 10) + (second as u32 - 0xDC00);
            return char::from_u32(combined);
        }
        char::from_u32(first as u32)
    }

    fn hex4(&mut self) -> Option<u16> {
        let text = std::str::from_utf8(self.bytes.get(self.at + 1..self.at + 5)?).ok()?;
        let value = u16::from_str_radix(text, 16).ok()?;
        self.at += 4;
        Some(value)
    }

    fn number(&mut self) -> Option<Json> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')) {
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).ok()?;
        text.parse().ok().map(Json::Number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_shape_of_a_reply() {
        let doc = Json::parse(
            r#"{"resultCount": 2, "results": [
                 {"artistName": "Boards of Canada", "collectionName": "Geogaddi",
                  "artworkUrl100": "https://example.test/100x100bb.jpg",
                  "releaseDate": "2002-02-18T08:00:00Z", "explicit": false},
                 {"artistName": "Autechre", "collectionName": "Amber"}
               ]}"#,
        )
        .unwrap();

        assert_eq!(doc.get("resultCount").unwrap().as_number(), Some(2.0));
        let results = doc.list("results");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].text("collectionName"), Some("Geogaddi"));
        assert_eq!(results[1].text("artistName"), Some("Autechre"));
        assert_eq!(results[1].text("artworkUrl100"), None);
        assert_eq!(results[0].get("explicit"), Some(&Json::Bool(false)));
    }

    #[test]
    fn escapes_come_out_as_the_characters_they_stand_for() {
        let doc = Json::parse(r#"{"a": "say \"hello\"", "b": "one\/two", "c": "line\nbreak"}"#)
            .unwrap();
        assert_eq!(doc.text("a"), Some("say \"hello\""));
        assert_eq!(doc.text("b"), Some("one/two"));
        assert_eq!(doc.text("c"), Some("line\nbreak"));
    }

    #[test]
    fn unicode_escapes_and_their_pairs() {
        let doc = Json::parse(
            r#"{"a": "\u00e4", "b": "\u5742\u672c", "c": "\ud83d\udcbf", "d": "abc"}"#,
        )
        .unwrap();
        assert_eq!(doc.text("a"), Some("ä"));
        assert_eq!(doc.text("b"), Some("坂本"));
        assert_eq!(doc.text("c"), Some("💿"));
        assert_eq!(doc.text("d"), Some("abc"));
    }

    #[test]
    fn characters_that_were_never_escaped_survive() {
        let doc = Json::parse(r#"{"a": "坂本龍一", "b": "Café"}"#).unwrap();
        assert_eq!(doc.text("a"), Some("坂本龍一"));
        assert_eq!(doc.text("b"), Some("Café"));
    }

    #[test]
    fn numbers_of_every_shape() {
        let doc = Json::parse(r#"{"a": 0, "b": -12, "c": 1.5, "d": 2e3, "e": -0.5e-2}"#).unwrap();
        assert_eq!(doc.get("a").unwrap().as_number(), Some(0.0));
        assert_eq!(doc.get("b").unwrap().as_number(), Some(-12.0));
        assert_eq!(doc.get("c").unwrap().as_number(), Some(1.5));
        assert_eq!(doc.get("d").unwrap().as_number(), Some(2000.0));
        assert_eq!(doc.get("e").unwrap().as_number(), Some(-0.005));
    }

    #[test]
    fn empty_containers_are_containers() {
        assert_eq!(Json::parse("{}"), Some(Json::Object(Vec::new())));
        assert_eq!(Json::parse("[]"), Some(Json::Array(Vec::new())));
        assert_eq!(Json::parse(r#"{"a": [], "b": {}}"#).unwrap().list("a").len(), 0);
    }

    #[test]
    fn broken_input_reads_as_nothing() {
        for bad in [
            "",
            "{",
            "{\"a\": }",
            "{\"a\" 1}",
            "[1, 2",
            "{\"a\": 1} trailing",
            "\"unterminated",
            r#"{"a": "\q"}"#,
            r#"{"a": "\ud83d"}"#,
        ] {
            assert_eq!(Json::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn whitespace_anywhere_is_allowed() {
        let doc = Json::parse(" {\n\t\"a\" : [ 1 , 2 ] \r\n} ").unwrap();
        assert_eq!(doc.list("a").len(), 2);
    }
}
