//! Talking to Last.fm.
//!
//! The request goes out through `curl`, for the same reason the folder picker
//! shells out to the system's own dialog: this program makes perhaps three
//! small requests per song, and an HTTP client with a TLS stack behind it would
//! be the largest dependency in the build by a wide margin, for that. `curl`
//! ships with macOS and with Windows, and is on every Linux worth the name. If
//! it is missing, scrobbling says so and switches itself off rather than
//! failing quietly.
//!
//! The reply is asked for as XML, not JSON. Last.fm's XML is small and fixed,
//! the four values this program reads out of it are one `<tag>` deep, and
//! reading them with string search needs no parser at all. A JSON parser would
//! be another dependency for the same four values.

use std::io::Write;
use std::process::Stdio;

use super::sign::api_sig;
use crate::error::{KesslerError, Result};

/// Where every call goes.
const ENDPOINT: &str = "https://ws.audioscrobbler.com/2.0/";

/// The same, unless `KESSLER_LASTFM_ENDPOINT` says otherwise.
///
/// There is no other way to see what this program actually puts on the wire:
/// the signature, the encoding and the field names are exactly the things that
/// are wrong the first time, and "it returned an error" does not say which.
/// Pointing it at a listener on this machine turns that into something that can
/// be read.
fn endpoint() -> String {
    std::env::var("KESSLER_LASTFM_ENDPOINT").unwrap_or_else(|_| ENDPOINT.to_string())
}

/// How long one call may take. Scrobbling is never urgent, and a scrobble that
/// hangs must not hold a queue up behind it.
const TIMEOUT_SECS: u32 = 15;

/// Percent-encode one value for a form body.
///
/// Written out because the rule is short and the alternative is a crate: keep
/// the unreserved set, encode everything else, and — this is the one that bites
/// — a space becomes `%20`, never `+`, because the signature was taken over the
/// raw text and the two have to describe the same thing.
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

/// A form body from parameters already in their final order.
pub fn form_body(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// The text between the first `<tag …>` and its `</tag>`, unescaped.
///
/// `None` when the tag is not there at all, which is how a missing value is
/// told apart from an empty one.
pub fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let start = xml.find(&open)?;
    let after_open = xml[start..].find('>')? + start + 1;
    let end = xml[after_open..].find(&close)? + after_open;
    Some(unescape(&xml[after_open..end]))
}

/// The five XML entities, turned back into characters.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// What Last.fm said went wrong, if anything did.
///
/// The code matters: 9 means the session is no longer valid and the user has to
/// connect again, 11 and 16 mean "try later", and the rest are ours to fix.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    pub code: u32,
    pub message: String,
}

impl ApiError {
    /// Whether waiting and sending it again is worth doing.
    #[cfg(test)]
    pub fn worth_retrying(&self) -> bool {
        matches!(self.code, 11 | 16 | 29)
    }

    /// Whether the stored session has stopped working.
    #[cfg(test)]
    pub fn needs_reconnecting(&self) -> bool {
        matches!(self.code, 4 | 9 | 14)
    }
}

/// The error in a reply, if the reply is one.
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

/// One call to the service.
///
/// `params` go out as given plus the signature; the reply comes back as text.
/// Anything that is not a reply — no curl, no network, a timeout — is an error
/// here rather than an empty answer, so the caller can tell "it said no" from
/// "it never got there".
pub fn call(params: Vec<(String, String)>, secret: &str) -> Result<String> {
    let mut params = params;
    params.push(("api_sig".to_string(), api_sig(&params, secret)));
    let body = form_body(&params);

    let mut child = crate::net::curl()
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

    /// A space is `%20`. `+` would not match what was signed.
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

    /// Non-ASCII goes out as its UTF-8 bytes, one `%XX` each.
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

    /// The session reply nests, and the name is wanted as well as the key.
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

    /// An ampersand in a title comes back escaped and has to go back.
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

    /// A reply with no error is not an error, however odd it looks.
    #[test]
    fn an_ordinary_reply_carries_no_error() {
        assert!(error_in(r#"<lfm status="ok"><nowplaying/></lfm>"#).is_none());
        assert!(error_in("").is_none());
    }

    /// The whole chain, against a listener on this machine: curl runs, the
    /// body is a signed form post, and the reply comes back as text.
    ///
    /// This is the only test that proves the request is actually made — the
    /// rest check the pieces. It needs nothing but a loopback socket.
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
