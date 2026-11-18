use super::{Episode, Show};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Query {
    pub term: String,
    words: Vec<String>,
    address: bool,
}

impl Query {
    pub fn parse(typed: &str) -> Option<Query> {
        let term = typed.trim();
        if term.is_empty() {
            return None;
        }
        let words = folded(term).split_whitespace().map(str::to_string).collect();
        Some(Query { term: term.to_string(), words, address: super::is_address(term) })
    }

    pub fn at_home(&self) -> bool {
        !self.address
    }

    pub fn show_matches(&self, show: &Show) -> bool {
        self.all_in(&[&show.title, &show.author])
    }

    pub fn episode_matches(&self, episode: &Episode, show: &Show) -> bool {
        self.all_in(&[&episode.title, &episode.about, &show.title])
    }

    fn all_in(&self, texts: &[&str]) -> bool {
        let texts: Vec<String> = texts.iter().map(|t| folded(t)).collect();
        self.words.iter().all(|w| texts.iter().any(|t| t.contains(w.as_str())))
    }
}

fn folded(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfkd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).collect::<String>().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(title: &str, author: &str) -> Show {
        Show { title: title.into(), author: author.into(), feed: format!("https://example.org/{title}"), artwork: String::new() }
    }

    fn episode(title: &str, about: &str) -> Episode {
        Episode { title: title.into(), about: about.into(), ..Default::default() }
    }

    #[test]
    fn words_look_everywhere_as_typed() {
        let q = Query::parse("  lage nation ").unwrap();
        assert_eq!(q.term, "lage nation");
        assert!(q.at_home());
        let q = Query::parse("teil 2: das ende").unwrap();
        assert_eq!(q.term, "teil 2: das ende");
        assert!(q.at_home());
    }

    #[test]
    fn nothing_typed_is_no_search() {
        assert!(Query::parse("").is_none());
        assert!(Query::parse("   ").is_none());
    }

    #[test]
    fn an_address_goes_to_the_directory_alone() {
        let q = Query::parse("https://example.org/feed.xml").unwrap();
        assert!(!q.at_home());
        assert_eq!(q.term, "https://example.org/feed.xml");
    }

    #[test]
    fn every_word_has_to_be_there_anywhere() {
        let q = Query::parse("nation lage").unwrap();
        assert!(q.show_matches(&show("Lage der Nation", "")));
        assert!(!q.show_matches(&show("Lage der Welt", "")));
        assert!(Query::parse("muller").unwrap().show_matches(&show("Gespräche", "Anna Müller")));
        let ldn = show("Lage der Nation", "");
        let q = Query::parse("klima").unwrap();
        assert!(q.episode_matches(&episode("Folge 12", "Heute über das Klima."), &ldn));
        assert!(!q.episode_matches(&episode("Folge 13", "Über Wahlen."), &ldn));
        assert!(Query::parse("nation klima").unwrap().episode_matches(&episode("Folge 12", "Heute über das Klima."), &ldn));
        assert!(!Query::parse("welt klima").unwrap().episode_matches(&episode("Folge 12", "Heute über das Klima."), &ldn));
    }
}
