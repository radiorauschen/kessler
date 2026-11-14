use std::io::Write;
use std::process::{Command, Stdio};

use super::sign::api_sig;
use crate::error::{KesslerError, Result};

const ENDPOINT: &str = "https://ws.audioscrobbler.com/2.0/";

fn endpoint() -> String {
    std::env::var("KESSLER_LASTFM_ENDPOINT").unwrap_or_else(|_| ENDPOINT.to_string())
}

const TIMEOUT_SECS: u32 = 15;

pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn form_body(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let start = xml.find(&open)?;
    let after_open = xml[start..].find('>')? + start + 1;
    let end = xml[after_open..].find(&close)? + after_open;
    Some(unescape(&xml[after_open..end]))
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    pub code: u32,
    pub message: String,
}

impl ApiError {
    #[cfg(test)]
    pub fn worth_retrying(&self) -> bool {
        matches!(self.code, 11 | 16 | 29)
    }

    #[cfg(test)]
    pub fn needs_reconnecting(&self) -> bool {
        matches!(self.code, 4 | 9 | 14)
    }
}

pub fn error_in(xml: &str) -> Option<ApiError> {
    if !xml.contains("status=\"failed\"") && !xml.contains("<error") {
        return None;
    }
    let start = xml.find("<error")?;
    let code = xml[start..]
        .find("code=\"")
        .map(|at| start + at + 6)
        .and_then(|from| {
            let rest = &xml[from..];
            rest.find('"').map(|to| rest[..to].to_string())
        })
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(0);
    let message = tag(xml, "error").unwrap_or_default();
    Some(ApiError {
        code,
        message: message.trim().to_string(),
    })
}

pub fn call(params: Vec<(String, String)>, secret: &str) -> Result<String> {
    let mut params = params;
    params.push(("api_sig".to_string(), api_sig(&params, secret)));
    let body = form_body(&params);

    let mut child = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail-with-body",
            "--max-time",
            &TIMEOUT_SECS.to_string(),
            "--request",
            "POST",
            "--header",
            "Content-Type: application/x-www-form-urlencoded",
            "--data-binary",
            "@-",
            &endpoint(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            KesslerError::Lastfm(tr!("curl liess sich nicht starten ({}) — ohne curl kein last.fm",
                e
            ))
        })?;

    child
        .stdin
        .as_mut()
        .ok_or_else(|| KesslerError::Lastfm(tr!("curl nahm die anfrage nicht an").into()))?
        .write_all(body.as_bytes())?;

    let output = child
        .wait_with_output()
        .map_err(|e| KesslerError::Lastfm(tr!("curl brach ab: {}", e)))?;

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.status.success() && text.trim().is_empty() {
        let why = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(KesslerError::Lastfm(tr!("last.fm nicht erreichbar: {}",
            if why.is_empty() { tr!("keine antwort").to_string() } else { why }
        )));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_space_is_not_a_plus() {
        assert_eq!(encode("xtal two"), "xtal%20two");
        assert_eq!(encode("a+b"), "a%2Bb");
        assert_eq!(encode("Boards of Canada"), "Boards%20of%20Canada");
    }

    #[test]
    fn the_unreserved_set_is_left_alone() {
        assert_eq!(encode("aZ09-_.~"), "aZ09-_.~");
    }

    #[test]
    fn non_ascii_is_encoded_per_byte() {
        assert_eq!(encode("ö"), "%C3%B6");
        assert_eq!(encode("坂本"), "%E5%9D%82%E6%9C%AC");
    }

    #[test]
    fn a_body_is_the_pairs_joined() {
        let params = vec![
            ("method".to_string(), "track.scrobble".to_string()),
            ("artist".to_string(), "Aphex Twin".to_string()),
        ];
        assert_eq!(
            form_body(&params),
            "method=track.scrobble&artist=Aphex%20Twin"
        );
    }

    #[test]
    fn a_value_is_read_out_of_its_tag() {
        let xml = r#"<lfm status="ok"><token>abc123</token></lfm>"#;
        assert_eq!(tag(xml, "token").as_deref(), Some("abc123"));
        assert_eq!(tag(xml, "key"), None);
    }

    #[test]
    fn a_session_is_read_out_of_its_reply() {
        let xml = r#"<?xml version="1.0"?>
            <lfm status="ok">
              <session>
                <name>somebody</name>
                <key>00112233445566778899aabbccddeeff</key>
                <subscriber>0</subscriber>
              </session>
            </lfm>"#;
        assert_eq!(tag(xml, "name").as_deref(), Some("somebody"));
        assert_eq!(
            tag(xml, "key").as_deref(),
            Some("00112233445566778899aabbccddeeff")
        );
        assert!(error_in(xml).is_none());
    }

    #[test]
    fn entities_are_turned_back_into_characters() {
        let xml = "<lfm><track>rock &amp; roll &lt;live&gt;</track></lfm>";
        assert_eq!(
            tag(xml, "track").as_deref(),
            Some("rock & roll <live>")
        );
    }

    #[test]
    fn a_failure_is_recognised_with_its_code() {
        let xml = r#"<?xml version="1.0"?>
            <lfm status="failed">
              <error code="9">Invalid session key - Please re-authenticate</error>
            </lfm>"#;
        let error = error_in(xml).unwrap();
        assert_eq!(error.code, 9);
        assert!(error.message.contains("Invalid session key"));
        assert!(error.needs_reconnecting());
        assert!(!error.worth_retrying());
    }

    #[test]
    fn a_service_that_is_busy_is_worth_another_go() {
        let xml = r#"<lfm status="failed"><error code="16">temporarily unavailable</error></lfm>"#;
        let error = error_in(xml).unwrap();
        assert!(error.worth_retrying());
        assert!(!error.needs_reconnecting());
    }

    #[test]
    fn an_ordinary_reply_carries_no_error() {
        assert!(error_in(r#"<lfm status="ok"><nowplaying/></lfm>"#).is_none());
        assert!(error_in("").is_none());
    }

    #[test]
    fn a_call_goes_out_as_a_signed_form_post() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpListener;

        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(_) => return,
        };
        let port = listener.local_addr().unwrap().port();

        let caught = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(&stream);
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();

            let mut length = 0usize;
            let mut content_type = String::new();
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                let header = header.trim_end().to_string();
                if header.is_empty() {
                    break;
                }
                let lower = header.to_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
                if let Some(value) = lower.strip_prefix("content-type:") {
                    content_type = value.trim().to_string();
                }
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();

            let reply = "<?xml version=\"1.0\"?><lfm status=\"ok\"><token>abc</token></lfm>";
            let mut stream = &stream;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                reply.len(),
                reply
            );
            let _ = stream.flush();
            (first, content_type, String::from_utf8_lossy(&body).to_string())
        });

        std::env::set_var("KESSLER_LASTFM_ENDPOINT", format!("http://127.0.0.1:{port}/"));
        let reply = call(
            vec![
                ("method".to_string(), "auth.getToken".to_string()),
                ("api_key".to_string(), "KEY".to_string()),
                ("track".to_string(), "xtal two".to_string()),
            ],
            "SECRET",
        );
        std::env::remove_var("KESSLER_LASTFM_ENDPOINT");

        let Ok(reply) = reply else {
            let _ = caught.join();
            return;
        };
        assert_eq!(tag(&reply, "token").as_deref(), Some("abc"));

        let (line, content_type, body) = caught.join().unwrap();
        assert!(line.starts_with("POST "), "not a post: {line}");
        assert!(content_type.starts_with("application/x-www-form-urlencoded"));
        assert!(body.contains("method=auth.getToken"));
        assert!(body.contains("track=xtal%20two"), "body was {body}");

        let expected = api_sig(
            &[
                ("method".to_string(), "auth.getToken".to_string()),
                ("api_key".to_string(), "KEY".to_string()),
                ("track".to_string(), "xtal two".to_string()),
            ],
            "SECRET",
        );
        assert!(body.ends_with(&format!("api_sig={expected}")), "body was {body}");
    }
}
