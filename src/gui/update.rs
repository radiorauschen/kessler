use std::sync::mpsc;

use crate::json::Json;

fn endpoint() -> String {
    std::env::var("KESSLER_UPDATE_ENDPOINT")
        .unwrap_or_else(|_| "https://api.github.com/repos/radiorauschen/kessler/releases/latest".to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Newer { version: String, page: String },
    Current,
    Unseen(String),
}

pub struct Check {
    asking: Option<mpsc::Receiver<Answer>>,
    pub answer: Option<Answer>,
}

impl Check {
    pub fn start() -> Check {
        let (tell, asking) = mpsc::channel();
        std::thread::Builder::new()
            .name("kessler-update".into())
            .spawn(move || {
                let _ = tell.send(ask());
            })
            .ok();
        Check { asking: Some(asking), answer: None }
    }

    pub fn tend(&mut self) -> bool {
        if let Some(asking) = &self.asking {
            match asking.try_recv() {
                Ok(answer) => {
                    self.answer = Some(answer);
                    self.asking = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.answer = Some(Answer::Unseen(String::new()));
                    self.asking = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        self.asking.is_some()
    }
}

fn ask() -> Answer {
    match crate::library::coverart::fetch_text(&endpoint()) {
        Ok(text) => match Json::parse(&text) {
            Some(doc) => read(&doc, env!("CARGO_PKG_VERSION")),
            None => Answer::Unseen(String::new()),
        },
        Err(e) => Answer::Unseen(e.to_string()),
    }
}

fn read(doc: &Json, built: &str) -> Answer {
    let Some(tag) = doc.text("tag_name") else {
        return Answer::Unseen(String::new());
    };
    let version = tag.trim().trim_start_matches(['v', 'V']).to_string();
    let page = doc.text("html_url").unwrap_or("").to_string();
    match (numbers(&version), numbers(built)) {
        (Some(out), Some(here)) if out > here => Answer::Newer { version, page },
        (Some(_), Some(_)) => Answer::Current,
        _ => Answer::Unseen(String::new()),
    }
}

fn numbers(version: &str) -> Option<Vec<u64>> {
    version.split('.').map(|part| part.trim().parse().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_release_is_told_by_its_numbers_not_its_letters() {
        let doc = |tag: &str| Json::parse(&format!(r#"{{"tag_name":"{tag}","html_url":"https://x/r"}}"#)).unwrap();
        assert_eq!(
            read(&doc("v1.4.140"), "1.4.131"),
            Answer::Newer { version: "1.4.140".into(), page: "https://x/r".into() }
        );
        assert!(matches!(read(&doc("v1.10.0"), "1.9.9"), Answer::Newer { .. }));
        assert_eq!(read(&doc("v1.3.62"), "1.4.131"), Answer::Current);
        assert_eq!(read(&doc("v1.4.131"), "1.4.131"), Answer::Current);
        assert!(matches!(read(&doc("nightly"), "1.4.131"), Answer::Unseen(_)));
    }
}
