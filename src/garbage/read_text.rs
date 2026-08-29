use super::generator::{
    JOIN_AFTER, JOIN_BEFORE, Next, Node, SENTENCE_ENDINGS, State, Substring, is_ending,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::iter::Peekable;
use std::num::NonZeroUsize;
use std::path::Path;
use std::str::CharIndices;
use std::{fmt, fs::File, io, io::Read};

type Parsed = (String, Box<[Node]>);

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

pub fn read_from_files<P: AsRef<Path>>(paths: &[P]) -> Result<Parsed, ParseError> {
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
    interned: HashMap<Cow<'a, str>, Substring>,
}

impl<'a> ParseState<'a> {
    fn intern(&mut self, substring: Capitalized<'a>) -> Substring {
        let maybe_downcase = substring.downcase();
        *self
            .interned
            .entry(maybe_downcase.clone())
            .or_insert_with(|| {
                let start = self.compressed.len();
                let end = start + maybe_downcase.len();
                self.compressed.push_str(maybe_downcase.as_ref());
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

        let mut states = self
            .map
            .keys()
            // .filter(|(s1, _s2)| !s1.of(&self.compressed).starts_with(JOIN_BEFORE))
            .copied()
            .collect::<Vec<_>>()
            .into_boxed_slice();
        states.sort();
        assert_eq!(states[0], (Substring(0, 0), Substring(0, 0)));

        let indices: HashMap<_, _> = states.iter().copied().zip(0..).collect();

        let mut nodes = states
            .iter()
            .map(|state| {
                let choices = self
                    .map
                    .get(&state)
                    .expect("every element of map.keys() should be in map")
                    .iter()
                    .map(|&word| {
                        let next = indices.get(&(state.1, word)).copied().map(|n| {
                            NonZeroUsize::new(n).expect("nothing should link back to the 0 index")
                        });
                        Next { word, next }
                    })
                    .collect::<Box<[Next]>>();
                assert!(!choices.is_empty());
                Node { choices }
            })
            .collect::<Box<[Node]>>();

        // Zeroth node is every word that starts a new sentence
        for (state, nexts) in self.map.iter().filter(|((_prev1, prev2), _nexts)| {
            SENTENCE_ENDINGS.contains(&prev2.of(&self.compressed))
        }) {
            nodes[0].choices = nodes[0]
                .choices
                .iter()
                .copied()
                .chain(nexts.iter().map(|&word| {
                    Next {
                        word,
                        next: Some(
                            indices
                                .get(state)
                                .copied()
                                .map(|n| {
                                    NonZeroUsize::new(n)
                                        .expect("nothing should link back to the 0 index")
                                })
                                .expect("every key in map is a key in indices"),
                        ),
                    }
                }))
                .collect::<Vec<_>>()
                .into_boxed_slice();
        }

        Ok((self.compressed, nodes))
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Capitalized<'a> {
    Normal(&'a str),
    Proper(&'a str),
}

impl<'a> Default for Capitalized<'a> {
    fn default() -> Self {
        Capitalized::Normal("")
    }
}

impl<'a> Capitalized<'a> {
    fn downcase(self) -> Cow<'a, str> {
        match self {
            Capitalized::Normal(s) => Cow::Owned(s.to_lowercase()),
            Capitalized::Proper(s) => Cow::Borrowed(s),
        }
    }
}

struct Substrings<'a> {
    leading_nulls: u8,
    source: &'a str,
    start_of_sentence: bool,
    inner: Peekable<CharIndices<'a>>,
}

impl<'a> Substrings<'a> {
    fn new(s: &'a str) -> Self {
        Substrings {
            leading_nulls: 2,
            source: s,
            start_of_sentence: true,
            inner: s.char_indices().peekable(),
        }
    }

    fn windows(self) -> SubstringsWindows<'a> {
        SubstringsWindows::new(self)
    }
}

fn split_on_punct(c: char) -> bool {
    JOIN_BEFORE.contains(&c) || JOIN_AFTER.contains(&c)
}

impl<'a> Iterator for Substrings<'a> {
    type Item = Capitalized<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.leading_nulls > 0 {
            if self.inner.peek().is_none() {
                return None;
            }
            self.leading_nulls -= 1;
            return Some(Capitalized::Normal(""));
        }

        let mut is_punct = false;

        let a = loop {
            let (idx, c) = self.inner.next()?;
            if !c.is_whitespace() {
                if split_on_punct(c) {
                    is_punct = true;
                }
                // println!("starting at char {c:?} - punct: {is_punct}");
                break idx;
            }
        };

        let b = loop {
            match self.inner.peek().copied() {
                Some((idx, c)) => {
                    // println!("checking {:?}", &self.source[a..idx]);
                    if is_punct && !split_on_punct(c) {
                        // println!("punctuation ends at {c:?}");
                        break idx;
                    }
                    if !is_punct && split_on_punct(c) {
                        // println!("word ends at punct {c:?}");
                        break idx;
                    }

                    self.inner.next(); // use up the peek

                    if c.is_whitespace() {
                        // println!("word ends at whitespace");
                        break idx;
                    }
                }
                None => {
                    break self.source.len();
                }
            }
        };

        let word = &self.source[a..b];

        // Guess if word should preserve its capitalization
        let capitalized = if word
            .chars()
            .skip(if self.start_of_sentence { 1 } else { 0 })
            .any(char::is_uppercase)
        {
            Capitalized::Proper(word)
        } else {
            Capitalized::Normal(word)
        };

        // Mark start of sentence for the next word if this word ends a sentence,
        // or if we already ended a sentence and this word is just punctuation.
        self.start_of_sentence = (self.start_of_sentence && is_punct) || is_ending(word);

        Some(capitalized)
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
    window: [Capitalized<'a>; 3],
    state: WindowState,
}

impl<'a> SubstringsWindows<'a> {
    fn new(mut inner: Substrings<'a>) -> Self {
        let mut window = [Capitalized::default(); 3];
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
    type Item = (Capitalized<'a>, Capitalized<'a>, Capitalized<'a>);

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
    use super::{Capitalized, Next, Node, Substring, Substrings, read_from_strings};
    use pretty_assertions::assert_eq;
    use std::num::NonZero;

    #[test]
    fn substrings_parses_words() {
        let text = "this is some text";
        let substrings = Substrings::new(text)
            .map(Capitalized::downcase)
            .collect::<Vec<_>>();
        assert_eq!(substrings, vec!["", "", "this", "is", "some", "text"]);
    }

    #[test]
    fn substrings_splits_on_period() {
        let text = "this is. truly. text";
        let substrings = Substrings::new(text)
            .map(Capitalized::downcase)
            .collect::<Vec<_>>();
        assert_eq!(
            substrings,
            vec!["", "", "this", "is", ".", "truly", ".", "text"]
        );
    }

    #[test]
    fn substrings_splits_on_comma() {
        let text = "this is, truly, text";
        let substrings = Substrings::new(text)
            .map(Capitalized::downcase)
            .collect::<Vec<_>>();
        assert_eq!(
            substrings,
            vec!["", "", "this", "is", ",", "truly", ",", "text"]
        );
    }

    #[test]
    fn substrings_splits_on_semicolon() {
        let text = "this is; truly; text";
        let substrings = Substrings::new(text)
            .map(Capitalized::downcase)
            .collect::<Vec<_>>();
        assert_eq!(
            substrings,
            vec!["", "", "this", "is", ";", "truly", ";", "text"]
        );
    }

    #[test]
    fn substrings_splits_on_parens() {
        let text = "this is (truly) text";
        let substrings = Substrings::new(text)
            .map(Capitalized::downcase)
            .collect::<Vec<_>>();
        assert_eq!(
            substrings,
            vec!["", "", "this", "is", "(", "truly", ")", "text"]
        );
    }

    #[test]
    fn substrings_preserves_infix_hyphens() {
        let text = "this is very-cool text";
        let substrings = Substrings::new(text)
            .map(Capitalized::downcase)
            .collect::<Vec<_>>();
        assert_eq!(substrings, vec!["", "", "this", "is", "very-cool", "text"]);
    }

    #[test]
    fn substrings_downcases_ordinary_words() {
        let text = "This is cool. Truly it is.";
        let substrings = Substrings::new(text)
            .map(Capitalized::downcase)
            .collect::<Vec<_>>();
        assert_eq!(
            substrings,
            vec!["", "", "this", "is", "cool", ".", "truly", "it", "is", "."]
        );
    }

    #[test]
    fn substrings_preserves_capitalization_of_proper_nouns() {
        let text = "Test-Runner, Steve, is a good man. “Great.”";
        let substrings = Substrings::new(text)
            .map(Capitalized::downcase)
            .collect::<Vec<_>>();
        assert_eq!(
            substrings,
            vec![
                "",
                "",
                "Test-Runner",
                ",",
                "Steve",
                ",",
                "is",
                "a",
                "good",
                "man",
                ".",
                "“",
                "great",
                ".”"
            ]
        );
    }

    #[test]
    fn substrings_windows() {
        let text = "this is some text";
        let mut windows = Substrings::new(text).windows();
        // Incredibly difficult to compare to the underlying
        assert_eq!(
            windows.next(),
            Some((
                Capitalized::Normal(""),
                Capitalized::Normal(""),
                Capitalized::Normal("this")
            ))
        );
        assert_eq!(
            windows.next(),
            Some((
                Capitalized::Normal(""),
                Capitalized::Normal("this"),
                Capitalized::Normal("is")
            ))
        );
        assert_eq!(
            windows.next(),
            Some((
                Capitalized::Normal("this"),
                Capitalized::Normal("is"),
                Capitalized::Normal("some")
            ))
        );
        assert_eq!(
            windows.next(),
            Some((
                Capitalized::Normal("is"),
                Capitalized::Normal("some"),
                Capitalized::Normal("text")
            ))
        );
        assert_eq!(windows.next(), None);
    }

    #[test]
    fn substrings_windows_returns_none() {
        let text = "";
        let mut windows = Substrings::new(text).windows();
        assert_eq!(windows.next(), None);
    }

    #[test]
    fn read() {
        let texts = &["this is a string", "this is so cool", "cool beans babe"];
        let (text, nodes) = read_from_strings(texts).unwrap();
        assert_eq!(text, "thisisastringsocoolbeansbabe");
        const _START_: Substring = Substring(0, 0);
        const THIS: Substring = Substring(0, 4);
        const IS: Substring = Substring(4, 6);
        const A: Substring = Substring(6, 7);
        const STRING: Substring = Substring(7, 13);
        const SO: Substring = Substring(13, 15);
        const COOL: Substring = Substring(15, 19);
        const BEANS: Substring = Substring(19, 24);
        const BABE: Substring = Substring(24, 28);
        // assert_eq!(
        //     map,
        //     HashMap::from([
        //         ((_START_, _START_), vec![THIS, THIS, COOL].into()),
        //         ((_START_, THIS), vec![IS, IS].into()),
        //         ((THIS, IS), vec![A, SO].into()),
        //         ((IS, A), vec![STRING].into()),
        //         ((IS, SO), vec![COOL].into()),
        //         ((_START_, COOL), vec![BEANS].into()),
        //         ((COOL, BEANS), vec![BABE].into()),
        //     ])
        // );
        // assert_eq!(
        //     &*states,
        //     &[
        //         (_START_, _START_),
        //         (_START_, THIS),
        //         (_START_, COOL),
        //         (THIS, IS),
        //         (IS, A),
        //         (IS, SO),
        //         (COOL, BEANS),
        //     ]
        // );

        assert_eq!(
            &*nodes,
            &[
                // (0) <start>
                Node {
                    choices: vec![
                        Next {
                            word: THIS,
                            next: Some(NonZero::new(1).unwrap()) // -> is
                        },
                        Next {
                            word: THIS,
                            next: Some(NonZero::new(1).unwrap()) // -> is
                        },
                        Next {
                            word: COOL,
                            next: Some(NonZero::new(2).unwrap()) // -> beans
                        }
                    ]
                    .into()
                },
                // (1) <start> this ->
                Node {
                    choices: vec![
                        Next {
                            word: IS,
                            next: Some(NonZero::new(3).unwrap()) // -> a
                        },
                        Next {
                            word: IS,
                            next: Some(NonZero::new(3).unwrap()) // -> so
                        }
                    ]
                    .into()
                },
                // (2) <start> cool ->
                Node {
                    choices: vec![Next {
                        word: BEANS,
                        next: Some(NonZero::new(6).unwrap()) // babe
                    }]
                    .into()
                },
                // (3) this is ->
                Node {
                    choices: vec![
                        Next {
                            word: A,
                            next: Some(NonZero::new(4).unwrap())
                        },
                        Next {
                            word: SO,
                            next: Some(NonZero::new(5).unwrap())
                        }
                    ]
                    .into()
                },
                // (4) is a ->
                Node {
                    choices: vec![Next {
                        word: STRING,
                        next: None
                    }]
                    .into()
                },
                // (5) is so ->
                Node {
                    choices: vec![Next {
                        word: COOL,
                        next: None,
                    }]
                    .into()
                },
                // (6) cool beans ->
                Node {
                    choices: vec![Next {
                        word: BABE,
                        next: None,
                    }]
                    .into()
                },
            ]
        );
    }
}
