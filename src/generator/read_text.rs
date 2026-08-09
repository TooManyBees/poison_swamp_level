use crate::generator::{State, Substring};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::iter::Peekable;
use std::str::CharIndices;
use std::{fmt, fs::File, io, io::Read};

type Parsed = (String, HashMap<State, Vec<Substring>>, Vec<State>);

pub fn read(text: &str) -> Result<Parsed, ParseError> {
    let mut parse_state = ParseState::default();
    parse_state.read(text);
    parse_state.finish()
}

pub fn read_from_strings(texts: &[&str]) -> Result<Parsed, ParseError> {
    let mut parse_state = ParseState::default();
    for text in texts {
        parse_state.read(text);
    }
    parse_state.finish()
}

pub fn read_from_files(paths: &[&str]) -> Result<Parsed, ParseError> {
    let mut text = String::new();
    let mut regions = Vec::with_capacity(paths.len());
    for path in paths {
        let start = text.len();
        let mut f = File::open(path)?;
        f.read_to_string(&mut text)?;
        regions.push(start..text.len());
    }
    let files: Vec<_> = regions.into_iter().map(|range| &text[range]).collect();
    read_from_strings(&files)
}

#[derive(Default)]
struct ParseState<'a> {
    compressed: String,
    map: HashMap<State, Vec<Substring>>,
    interned: HashMap<&'a str, Substring>,
}

impl<'a> ParseState<'a> {
    fn intern(&mut self, substring: &'a str) -> Substring {
        *self.interned.entry(substring).or_insert_with(|| {
            let start = self.compressed.len();
            let end = start + substring.len();
            self.compressed.push_str(substring);
            Substring(start, end)
        })
    }

    fn read(&mut self, text: &'a str) {
        let iter = Substrings::new(text);

        for (prev1, prev2, next) in iter.windows() {
            let prev1 = self.intern(prev1);
            let prev2 = self.intern(prev2);
            let next = self.intern(next);
            self.map.entry((prev1, prev2)).or_default().push(next);
        }
    }

    fn finish(mut self) -> Result<Parsed, ParseError> {
        if self.map.is_empty() {
            return Err(ParseError::NoContent);
        }
        self.compressed.shrink_to_fit();
        let mut states: Vec<_> = self.map.keys().copied().collect();
        states.sort_by(|(a1, a2), (b1, b2)| match a1.0.cmp(&b1.0) {
            Ordering::Equal => a2.0.cmp(&b2.0),
            ord => ord,
        });
        Ok((self.compressed, self.map, states))
    }
}

struct Substrings<'a> {
    source: &'a str,
    inner: Peekable<CharIndices<'a>>,
}

impl<'a> Substrings<'a> {
    fn new(s: &'a str) -> Self {
        Substrings {
            source: s,
            inner: s.char_indices().peekable(),
        }
    }

    fn windows(self) -> SubstringsWindows<'a> {
        SubstringsWindows::new(self)
    }
}

fn split_on_punct(c: char) -> bool {
    c != '-' && c.is_ascii_punctuation()
}

impl<'a> Iterator for Substrings<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let mut is_punct = false;

        let a = loop {
            let (idx, c) = self.inner.next()?;
            if !c.is_whitespace() {
                if split_on_punct(c) {
                    is_punct = true;
                }
                break idx;
            }
        };

        let b = loop {
            match self.inner.peek().copied() {
                Some((idx, c)) => {
                    if is_punct && !split_on_punct(c) {
                        break idx;
                    }
                    if !is_punct && split_on_punct(c) {
                        break idx;
                    }

                    self.inner.next(); // use up the peek

                    if c.is_whitespace() {
                        break idx;
                    }
                }
                None => {
                    break self.source.len();
                }
            }
        };

        Some(&self.source[a..b])
    }
}

#[derive(Copy, Clone, Debug)]
enum WindowState {
    At0,
    At1,
    At2,
}

impl WindowState {
    fn next(self) -> Self {
        match self {
            WindowState::At0 => WindowState::At1,
            WindowState::At1 => WindowState::At2,
            WindowState::At2 => WindowState::At0,
        }
    }

    fn as_index(self) -> usize {
        match self {
            WindowState::At0 => 0,
            WindowState::At1 => 1,
            WindowState::At2 => 2,
        }
    }
}

struct SubstringsWindows<'a> {
    inner: Substrings<'a>,
    window: [&'a str; 3],
    state: WindowState,
}

impl<'a> SubstringsWindows<'a> {
    fn new(mut inner: Substrings<'a>) -> Self {
        let mut window = [""; 3];
        window[0] = inner.next().unwrap_or_default();
        window[1] = inner.next().unwrap_or_default();

        SubstringsWindows {
            inner,
            window,
            state: WindowState::At2,
        }
    }
}

impl<'a> Iterator for SubstringsWindows<'a> {
    type Item = (&'a str, &'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        self.window[self.state.as_index()] = self.inner.next()?;
        let window = match self.state {
            WindowState::At0 => (self.window[1], self.window[2], self.window[0]),
            WindowState::At1 => (self.window[2], self.window[0], self.window[1]),
            WindowState::At2 => (self.window[0], self.window[1], self.window[2]),
        };
        self.state = self.state.next();
        Some(window)
    }
}

#[derive(Debug)]
pub enum ParseError {
    Io(io::Error),
    NoContent,
}

impl From<io::Error> for ParseError {
    fn from(e: io::Error) -> ParseError {
        ParseError::Io(e)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::Io(e) => e.fmt(f),
            ParseError::NoContent => write!(f, "The generator did not find any content."),
        }
    }
}

#[cfg(test)]
mod test {
    use super::{Substring, Substrings, read_from_strings};
    use std::collections::HashMap;

    #[test]
    fn substrings_parses_words() {
        let text = "this is some text";
        let substrings = Substrings::new(text).collect::<Vec<_>>();
        assert_eq!(substrings, vec!("this", "is", "some", "text"));
    }

    #[test]
    fn substrings_splits_on_period() {
        let text = "this is. truly. text";
        let substrings = Substrings::new(text).collect::<Vec<_>>();
        assert_eq!(substrings, vec!["this", "is", ".", "truly", ".", "text"]);
    }

    #[test]
    fn substrings_splits_on_comma() {
        let text = "this is, truly, text";
        let substrings = Substrings::new(text).collect::<Vec<_>>();
        assert_eq!(substrings, vec!["this", "is", ",", "truly", ",", "text"]);
    }

    #[test]
    fn substrings_splits_on_semicolon() {
        let text = "this is; truly; text";
        let substrings = Substrings::new(text).collect::<Vec<_>>();
        assert_eq!(substrings, vec!["this", "is", ";", "truly", ";", "text"]);
    }

    #[test]
    fn substrings_splits_on_parens() {
        let text = "this is (truly) text";
        let substrings = Substrings::new(text).collect::<Vec<_>>();
        assert_eq!(substrings, vec!["this", "is", "(", "truly", ")", "text"]);
    }

    #[test]
    fn substrings_preserves_infix_hyphens() {
        let text = "this is very-cool text";
        let substrings = Substrings::new(text).collect::<Vec<_>>();
        assert_eq!(substrings, vec!["this", "is", "very-cool", "text"]);
    }

    #[test]
    fn substrings_windows() {
        let text = "this is some text";
        let mut windows = Substrings::new(text).windows();
        assert_eq!(windows.next(), Some(("this", "is", "some")));
        assert_eq!(windows.next(), Some(("is", "some", "text")));
        assert_eq!(windows.next(), None);
    }

    #[test]
    fn substrings_windows_returns_none() {
        let text = "this is";
        let mut windows = Substrings::new(text).windows();
        assert_eq!(windows.next(), None);
    }

    #[test]
    fn read() {
        let texts = &["this is a string", "this is so cool", "cool beans babe"];
        let (text, map, states) = read_from_strings(texts).unwrap();
        assert_eq!(text, "thisisastringsocoolbeansbabe");
        assert_eq!(
            map,
            HashMap::from([
                // this is => [a, so]
                (
                    (Substring(0, 4), Substring(4, 6)),
                    vec![Substring(6, 7), Substring(13, 15)]
                ),
                // is a => [string]
                ((Substring(4, 6), Substring(6, 7)), vec![Substring(7, 13)]),
                // is so => [cool]
                (
                    (Substring(4, 6), Substring(13, 15)),
                    vec![Substring(15, 19)]
                ),
                // cool beans => [babe]
                (
                    (Substring(15, 19), Substring(19, 24)),
                    vec![Substring(24, 28)]
                ),
            ])
        );
        assert_eq!(
            states,
            vec![
                (Substring(0, 4), Substring(4, 6)),     // this is
                (Substring(4, 6), Substring(6, 7)),     // is a
                (Substring(4, 6), Substring(13, 15)),   // is so
                (Substring(15, 19), Substring(19, 24)), // cool beans
            ]
        )
    }
}
