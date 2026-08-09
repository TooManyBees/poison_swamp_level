use rand::{Rng, seq::IndexedRandom};
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::Read;
use std::str::CharIndices;

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
struct Substring(usize, usize);

impl Substring {
    fn of(self, s: &str) -> &str {
        &s[self.0..self.1]
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

struct Generator {
    text: String,
    map: HashMap<(Substring, Substring), Vec<Substring>>,
    keys: Vec<(Substring, Substring)>,
}

impl Generator {
    fn read(map: &mut HashMap<(Substring, Substring), Vec<Substring>>, text: &str) {
        let mut iter = Substrings::new(text);

        for (prev1, prev2, next) in iter.windows() {
            map.entry((prev1, prev2)).or_default().push(next);
        }
    }

    pub fn from_text(text: &str) -> Result<Generator, ReadGeneratorError> {
        let mut map = HashMap::new();
        Generator::read(&mut map, text);
        if map.is_empty() {
            return Err(ReadGeneratorError::NoContent);
        }
        let keys = map.keys().copied().collect();
        Ok(Generator {
            text: text.to_string(),
            map,
            keys,
        })
    }

    fn from_files(paths: &[&str]) -> Result<Generator, ReadGeneratorError> {
        let mut text = String::new();
        let mut regions = Vec::with_capacity(paths.len());
        for path in paths {
            let start = text.len();
            let mut f = File::open(path)?;
            f.read_to_string(&mut text)?;
            regions.push((start, text.len()));
        }

        let mut map = HashMap::new();
        for (start, end) in regions {
            Generator::read(&mut map, &text[start..end]);
        }

        if map.is_empty() {
            return Err(ReadGeneratorError::NoContent);
        }

        let keys = map.keys().copied().collect();

        Ok(Generator { text, map, keys })
    }

    fn generate<R: Rng>(&self, mut rng: R) -> Generated<'_, R> {
        let state = self.keys.choose(&mut rng).copied().unwrap_or_default();
        Generated {
            text: &self.text,
            map: &self.map,
            keys: &self.keys,
            rng,
            state,
        }
    }
}

#[derive(Debug)]
enum ReadGeneratorError {
    Io(io::Error),
    NoContent,
}

impl From<io::Error> for ReadGeneratorError {
    fn from(e: io::Error) -> ReadGeneratorError {
        ReadGeneratorError::Io(e)
    }
}

struct Generated<'a, R: Rng> {
    text: &'a str,
    map: &'a HashMap<(Substring, Substring), Vec<Substring>>,
    keys: &'a [(Substring, Substring)],
    rng: R,
    state: (Substring, Substring),
}

impl<'a, R: Rng> Iterator for Generated<'a, R> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.map.is_empty() {
            return None;
        }

        let next_word = self.state.0.of(self.text);

        let next_state = match self.map.get(&self.state) {
            Some(words) => words,
            None => {
                self.state = self.keys.choose(&mut self.rng).copied()?;
                &self.map[&self.state]
            }
        };

        let next = next_state.choose(&mut self.rng)?;
        self.state = (self.state.1, *next);

        Some(next_word)
    }
}

fn main() {
    let mut rng = rand::rng();
    let g = Generator::from_files(&vec!["./susan.sontag.notes.on.camp.txt"]).unwrap();
    let mut gg = g.generate(&mut rng);

    println!(
        "{} {} {}",
        gg.next().unwrap(),
        gg.next().unwrap(),
        gg.next().unwrap()
    );
}

#[cfg(test)]
mod test {
    use super::{Substring, Substrings};

    #[test]
    fn substrings() {
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
}
