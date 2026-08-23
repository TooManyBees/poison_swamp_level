use super::read_text::{ParseError, read, read_from_files, read_from_strings};
use rand::{Rng, RngExt, seq::IndexedRandom};
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::path::Path;

pub struct Corpus {
    text: String,
    map: HashMap<State, Vec<Substring>>,
    states: Vec<State>,
}

impl Corpus {
    pub fn from_string(text: &str) -> Result<Corpus, ParseError> {
        let (text, map, states) = read(text)?;
        Ok(Corpus { text, map, states })
    }

    pub fn from_strings(texts: &[&str]) -> Result<Corpus, ParseError> {
        let (text, map, states) = read_from_strings(texts)?;
        Ok(Corpus { text, map, states })
    }

    pub fn from_files<P: AsRef<Path>>(paths: &[P]) -> Result<Corpus, ParseError> {
        let (text, map, states) = read_from_files(paths)?;
        Ok(Corpus { text, map, states })
    }

    pub fn generator<R: Rng>(&self, mut rng: R) -> Generator<'_, R> {
        let state = self.states.choose(&mut rng).copied().unwrap_or_default();
        Generator {
            text: &self.text,
            map: &self.map,
            states: &self.states,
            rng,
            state,
        }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.text.len(), self.map.values().map(|vs| vs.len()).sum())
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

pub struct Generator<'a, R: Rng> {
    text: &'a str,
    map: &'a HashMap<State, Vec<Substring>>,
    states: &'a [State],
    rng: R,
    state: State,
}

impl<'a, R: Rng> Iterator for Generator<'a, R> {
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

static SENTENCE_ENDINGS: &'static [&'static str] =
    &[".", "!", "?", ".\"", "!\"", "?\"", ".”", "!”", "?”", ".)", "?)", "!)"];

pub fn is_ending<S: AsRef<str>>(s: S) -> bool {
    let s = s.as_ref().trim();
    if s.is_empty() {
        return true;
    }
    for ending in SENTENCE_ENDINGS {
        if s.ends_with(ending) {
            return true;
        }
    }
    false
}

pub static JOIN_BEFORE: &'static [char] = &['!', '"', ')', ',', '.', ':', ';', '?', '”'];
pub static JOIN_AFTER: &'static [char] = &['(', '“'];

fn whitespace_after_output(s: &str) -> bool {
    !s.ends_with(JOIN_AFTER)
}

fn whitespace_before_word(s: &str) -> bool {
    !s.starts_with(JOIN_BEFORE)
}

fn is_punct(s: &str) -> bool {
    s.chars().all(|c| char::is_ascii_punctuation(&c))
}

impl<'a, R: Rng> Generator<'a, R> {
    pub fn rng(&mut self) -> &mut R {
        &mut self.rng
    }

    pub fn paragraphs(
        &mut self,
        num_paragraphs: &RangeInclusive<usize>,
        num_words: &RangeInclusive<usize>,
    ) -> impl Iterator<Item = String> {
        let num_paragraphs = self.rng.random_range(num_paragraphs.clone());
        (0..num_paragraphs).map(|_| self.generate(num_words.clone()))
    }

    pub fn words(&mut self, range: RangeInclusive<usize>) -> impl Iterator<Item = &'a str> {
        let num_words = self.rng.random_range(range);
        self.filter(|w| w.chars().any(|c| !c.is_ascii_punctuation()))
            .take(num_words)
    }

    pub fn generate(&mut self, range: RangeInclusive<usize>) -> String {
        let mut output = String::new();

        let num_words = self.rng.random_range(range);

        if num_words > 0 {
            let mut must_capitalize = true;

            for word in self
                .skip_while(|w| w.starts_with(JOIN_BEFORE))
                .take(num_words)
            {
                must_capitalize = (must_capitalize && is_punct(word)) || is_ending(&output);
                let word = if must_capitalize {
                    must_capitalize = false;
                    capitalized(word)
                } else {
                    Cow::Borrowed(word)
                };

                if !output.is_empty()
                    && whitespace_before_word(&word)
                    && whitespace_after_output(&output)
                {
                    output.push(' ');
                }
                output.push_str(&word);
            }
        }

        if !is_ending(&output) {
            if output
                .chars()
                .last()
                .map(|c| char::is_ascii_punctuation(&c))
                .unwrap_or(false)
            {
                output.pop();
            }
            output.push('.');
        }
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

#[cfg(test)]
mod test {
    use super::Corpus;
    // use pretty_assertions::assert_eq;
    use rand_seeder::{Seeder, SipRng};

    #[test]
    fn generates_consistent_words() {
        let corpus = Corpus::from_files(&["susan.sontag.on.style.txt"]).unwrap();
        let mut rng: SipRng = Seeder::from("/some/predictable/path").into_rng();
        let generator = corpus.generator(&mut rng);

        let expected = [
            "new ( and any particular morality has its nourishment",
            ", art and the ethical be made . but the prevalence of",
            "genteel-moralistic judgments in contemporary literary (",
            "and sometimes commendable ) attitudes . we may become",
            "convinced of the world . i simply say that the notion of",
            "style is a useful notion because will is not so easy ,",
            "after all , seeking to defend the autonomy of art creates",
            "a world which is [the artist's] alone .” we can , in theory",
            ", the lure which engages consciousness in essentially formal",
            "processes of transformation . this act of",
        ]
        .into_iter()
        .flat_map(|line| line.split(' '))
        .collect::<Vec<_>>();

        let actual: Vec<&str> = generator.take(100).collect();

        assert_eq!(actual, expected);
    }
}
