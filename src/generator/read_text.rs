use crate::generator::{State, Substring};
use std::cmp::Ordering;
use std::collections::HashMap;
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
    fn intern(&mut self, substring: Substring, text: &'a str) -> Substring {
        *self.interned.entry(substring.of(text)).or_insert_with(|| {
            let start = self.compressed.len();
            let end = start + substring.1 - substring.0;
            self.compressed.push_str(substring.of(text));
            Substring(start, end)
        })
    }

    fn read(&mut self, text: &'a str) {
        let iter = Substrings::new(text);

        for (prev1, prev2, next) in iter.windows() {
            let prev1 = self.intern(prev1, text);
            let prev2 = self.intern(prev2, text);
            let next = self.intern(next, text);
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
    inner: CharIndices<'a>,
}

impl<'a> Substrings<'a> {
    fn new(s: &'a str) -> Self {
        Substrings {
            inner: s.char_indices(),
        }
    }

    fn windows(self) -> SubstringsWindows<'a> {
        SubstringsWindows::new(self)
    }
}

impl<'a> Iterator for Substrings<'a> {
    type Item = Substring;

    fn next(&mut self) -> Option<Self::Item> {
        let a = loop {
            let (idx, c) = self.inner.next()?;
            if !c.is_whitespace() {
                break idx;
            }
        };

        let b = loop {
            match self.inner.next() {
                Some((idx, c)) => {
                    if c.is_whitespace() {
                        break idx;
                    }
                }
                None => {
                    break self.inner.offset();
                }
            }
        };

        Some(Substring(a, b))
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
    window: [Substring; 3],
    state: WindowState,
}

impl<'a> SubstringsWindows<'a> {
    fn new(mut inner: Substrings<'a>) -> Self {
        let mut window = [Substring(0, 0); 3];
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
    type Item = (Substring, Substring, Substring);

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
        assert_eq!(
            substrings,
            vec!(
                Substring(0, 4),
                Substring(5, 7),
                Substring(8, 12),
                Substring(13, 17)
            )
        );
    }

    #[test]
    fn substrings_windows() {
        let text = "this is some text";
        let mut windows = Substrings::new(text).windows();
        assert_eq!(
            windows.next(),
            Some((Substring(0, 4), Substring(5, 7), Substring(8, 12)))
        );
        assert_eq!(
            windows.next(),
            Some((Substring(5, 7), Substring(8, 12), Substring(13, 17)))
        );
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
