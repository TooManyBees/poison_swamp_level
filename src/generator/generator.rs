use crate::generator::ParseState;
use rand::{Rng, seq::IndexedRandom};
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::Read;

pub struct Generator {
    text: String,
    map: HashMap<State, Vec<Substring>>,
    states: Vec<State>,
}

impl Generator {
    // pub fn from_text(text: &str) -> Result<Generator, ReadGeneratorError> {
    //     let mut map = HashMap::new();
    //     read(&mut map, text);
    //     if map.is_empty() {
    //         return Err(ReadGeneratorError::NoContent);
    //     }
    //     let states = map.keys().copied().collect();
    //     Ok(Generator {
    //         text: text.to_string(),
    //         map,
    //         states,
    //     })
    // }

    pub fn from_files(paths: &[&str]) -> Result<Generator, ReadGeneratorError> {
        let mut text = String::new();
        let mut regions = Vec::with_capacity(paths.len());
        for path in paths {
            let start = text.len();
            let mut f = File::open(path)?;
            f.read_to_string(&mut text)?;
            regions.push((start, text.len()));
        }

        let mut parse_state = ParseState::default();
        for (start, end) in regions {
            parse_state.read(&text[start..end]);
        }

        let (text, map) = parse_state.finish();

        if map.is_empty() {
            return Err(ReadGeneratorError::NoContent);
        }

        let states = map.keys().copied().collect();

        Ok(Generator { text, map, states })
    }

    pub fn generate<R: Rng>(&self, mut rng: R) -> Generated<'_, R> {
        let state = self.states.choose(&mut rng).copied().unwrap_or_default();
        Generated {
            text: &self.text,
            map: &self.map,
            states: &self.states,
            rng,
            state,
        }
    }
}

#[derive(Debug)]
pub enum ReadGeneratorError {
    Io(io::Error),
    NoContent,
}

impl From<io::Error> for ReadGeneratorError {
    fn from(e: io::Error) -> ReadGeneratorError {
        ReadGeneratorError::Io(e)
    }
}

impl fmt::Display for ReadGeneratorError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ReadGeneratorError::Io(e) => e.fmt(f),
            ReadGeneratorError::NoContent => write!(f, "The generator did not find any content."),
        }
    }
}

pub type State = (Substring, Substring);

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct Substring(pub(super) usize, pub(super) usize);

impl Substring {
    pub fn of(self, s: &str) -> &str {
        &s[self.0..self.1]
    }
}

pub struct Generated<'a, R: Rng> {
    text: &'a str,
    map: &'a HashMap<State, Vec<Substring>>,
    states: &'a [State],
    rng: R,
    state: State,
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
                self.state = self.states.choose(&mut self.rng).copied()?;
                &self.map[&self.state]
            }
        };

        let next = next_state.choose(&mut self.rng)?;
        self.state = (self.state.1, *next);

        Some(next_word)
    }
}
