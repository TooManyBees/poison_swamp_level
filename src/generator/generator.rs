use crate::generator::{ParseError, read, read_from_files, read_from_strings};
use rand::{Rng, seq::IndexedRandom};
use std::borrow::Cow;
use std::collections::HashMap;

pub struct Generator {
    text: String,
    map: HashMap<State, Vec<Substring>>,
    states: Vec<State>,
}

impl Generator {
    pub fn from_string(text: &str) -> Result<Generator, ParseError> {
        let (text, map, states) = read(text)?;
        Ok(Generator { text, map, states })
    }

    pub fn from_strings(texts: &[&str]) -> Result<Generator, ParseError> {
        let (text, map, states) = read_from_strings(texts)?;
        Ok(Generator { text, map, states })
    }

    pub fn from_files(paths: &[&str]) -> Result<Generator, ParseError> {
        let (text, map, states) = read_from_files(paths)?;
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

impl<'a, R: Rng> Generated<'a, R> {
    pub fn sentence(&mut self, length: usize) -> String {
        let mut output = String::new();

        if length > 0 {
            output.push_str(capitalized(self.next().unwrap()).as_ref());
            for word in self.take(length - 1) {
                output.push(' ');
                output.push_str(word);
            }
        }

        for ending in [".", "!", "?", ".\"", "!\"", "?\"", ".”", "!”", "?”"] {
            if output.ends_with(ending) {
                return output;
            }
        }

        output.push('.');
        output
    }
}

fn capitalized(word: &str) -> Cow<'_, str> {
    let first = word
        .chars()
        .nth(0)
        .expect("expected word to have at least 1 character");
    if first.is_ascii_uppercase() {
        Cow::Borrowed(word)
    } else {
        let new_word = std::iter::once(first.to_ascii_uppercase())
            .chain(word.chars().skip(1))
            .collect();
        Cow::Owned(new_word)
    }
}
