//! Literal, Unicode-aware matching; all ranges are UTF-8 byte offsets.
use regex::RegexBuilder;
use std::{ops::Range, sync::LazyLock};
use uuid::Uuid;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
}

fn word_char(c: char) -> bool {
    static WORD: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"\A\w\z").expect("static word regex"));
    WORD.is_match(c.encode_utf8(&mut [0; 4]))
}

pub fn find_matches(
    text: &str,
    query: &str,
    options: SearchOptions,
) -> Result<Vec<Range<usize>>, regex::Error> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = RegexBuilder::new(&regex::escape(query))
        .case_insensitive(!options.case_sensitive)
        .build()?;
    Ok(pattern
        .find_iter(text)
        .filter(|m| {
            !options.whole_word
                || (!text[..m.start()].chars().next_back().is_some_and(word_char)
                    && !text[m.end()..].chars().next().is_some_and(word_char))
        })
        .map(|m| m.range())
        .collect())
}

/// Substitution is always literal: `$1`, backslashes and newlines are not expanded.
pub fn replace_matches(text: &str, matches: &[Range<usize>], replacement: &str) -> Option<String> {
    let mut output = String::new();
    let mut end = 0;
    for range in matches {
        if range.start < end || range.start >= range.end || text.get(range.clone()).is_none() {
            return None;
        }
        output.push_str(&text[end..range.start]);
        output.push_str(replacement);
        end = range.end;
    }
    output.push_str(&text[end..]);
    Some(output)
}

struct Cache {
    region: Uuid,
    text: String,
    query: String,
    options: SearchOptions,
}

#[derive(Default)]
pub struct SearchState {
    pub open: bool,
    pub show_replace: bool,
    pub query: String,
    pub replacement: String,
    pub options: SearchOptions,
    pub current: Option<Range<usize>>,
    pub error: Option<String>,
    pub replaced: Option<usize>,
    pub request_query_focus: bool,
    pub matches: Vec<Range<usize>>,
    cache: Option<Cache>,
}

impl SearchState {
    pub fn refresh(&mut self, region: Uuid, text: &str) {
        if self.cache.as_ref().is_some_and(|c| {
            c.region == region
                && c.text == text
                && c.query == self.query
                && c.options == self.options
        }) {
            return;
        }
        let same_query = self.cache.as_ref().is_some_and(|c| {
            c.region == region && c.query == self.query && c.options == self.options
        });
        match find_matches(text, &self.query, self.options) {
            Ok(matches) => {
                self.matches = matches;
                self.error = None;
            }
            Err(error) => {
                self.matches.clear();
                self.error = Some(error.to_string());
            }
        }
        if !same_query
            || !self
                .matches
                .iter()
                .any(|r| Some(r) == self.current.as_ref())
        {
            self.current = self.matches.first().cloned();
        }
        self.replaced = None;
        self.cache = Some(Cache {
            region,
            text: text.into(),
            query: self.query.clone(),
            options: self.options,
        });
    }

    pub fn current_index(&self) -> Option<usize> {
        self.matches
            .iter()
            .position(|r| Some(r) == self.current.as_ref())
    }

    pub fn navigate(&mut self, backwards: bool) -> Option<Range<usize>> {
        if self.matches.is_empty() {
            self.current = None;
            return None;
        }
        let next = match self.current_index() {
            Some(i) if backwards => (i + self.matches.len() - 1) % self.matches.len(),
            Some(i) => (i + 1) % self.matches.len(),
            None if backwards => self.matches.len() - 1,
            None => 0,
        };
        self.current = Some(self.matches[next].clone());
        self.current.clone()
    }

    pub fn select_from(&mut self, byte: usize) -> Option<Range<usize>> {
        self.current = self
            .matches
            .iter()
            .find(|r| r.start >= byte)
            .or(self.matches.first())
            .cloned();
        self.current.clone()
    }
}
