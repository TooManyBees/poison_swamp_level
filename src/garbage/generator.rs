use super::read_text::{ParseError, read, read_from_files, read_from_strings};
use rand::{Rng, RngExt, seq::IndexedRandom};
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::{borrow::Cow, fmt, mem::size_of, path::Path};

pub struct Corpus {
    text: String,
    map: HashMap<State, Vec<Substring>>,
    states: Vec<State>,

    nodes: Vec<Node>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Node {
    pub next: Vec<Next>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Next {
    pub word: Substring,
    pub next: Option<usize>,
}

pub struct SizeData {
    text_bytes: usize,
    text_words: usize,
    map_keys: usize,
    map_bytes: usize,
    nodes_bytes: usize,
}

impl fmt::Display for SizeData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} words in {} bytes, map of {} states in {} bytes, nodes of {} bytes",
            self.text_words, self.text_bytes, self.map_keys, self.map_bytes, self.nodes_bytes
        )
    }
}

impl Corpus {
    pub fn from_string(text: &str) -> Result<Corpus, ParseError> {
        let (text, map, states, nodes) = read(text)?;
        Ok(Corpus {
            text,
            map,
            states,
            nodes,
        })
    }

    pub fn from_strings(texts: &[&str]) -> Result<Corpus, ParseError> {
        let (text, map, states, nodes) = read_from_strings(texts)?;
        Ok(Corpus {
            text,
            map,
            states,
            nodes,
        })
    }

    pub fn from_files<P: AsRef<Path>>(paths: &[P]) -> Result<Corpus, ParseError> {
        let (text, map, states, nodes) = read_from_files(paths)?;
        Ok(Corpus {
            text,
            map,
            states,
            nodes,
        })
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

    pub fn generator2<R: Rng>(&self, rng: R) -> Generator2<'_, R> {
        Generator2 {
            text: &self.text,
            nodes: &self.nodes,
            pos: 0,
            rng,
        }
    }

    pub fn size(&self) -> SizeData {
        let map_bytes = size_of::<HashMap<State, Vec<Substring>>>()
            + self
                .map
                .values()
                .map(|vs| {
                    size_of::<State>()
                        + size_of::<Vec<Substring>>()
                        + vs.len() * size_of::<Substring>()
                })
                .sum::<usize>();

        let state_bytes = size_of::<Vec<State>>() + self.states.len() * size_of::<State>();

        let nodes_bytes = size_of::<Vec<Node>>()
            + size_of::<Node>() * self.nodes.len()
            + self
                .nodes
                .iter()
                .map(|n| size_of::<Next>() * n.next.len())
                .sum::<usize>();

        SizeData {
            text_bytes: self.text.len(),
            text_words: 0,
            map_keys: self.map.len(),
            map_bytes: map_bytes + state_bytes,
            nodes_bytes,
        }
    }

    pub fn dump(&self) {
        use std::io::Write;
        let mut all_substrings: Vec<_> = self
            .map
            .values()
            .flat_map(|v| v.iter().map(|s| s.of(&self.text)))
            .collect();
        all_substrings.sort();
        all_substrings.dedup();
        let mut stdout = std::io::stdout().lock();
        for substr in all_substrings {
            let _ = write!(stdout, "{}\n", substr);
        }
    }
}

pub type State = (Substring, Substring);

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Substring(pub(super) usize, pub(super) usize);

impl Substring {
    pub fn of(self, s: &str) -> &str {
        &s[self.0..self.1]
    }
}

pub struct Generator2<'a, R: Rng> {
    text: &'a str,
    nodes: &'a [Node],
    pos: usize,
    rng: R,
}

impl<'a, R: Rng> Iterator for Generator2<'a, R> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.nodes.is_empty() {
            return None;
        }

        let node = self
            .nodes
            .get(self.pos)
            .or_else(|| self.nodes.choose(&mut self.rng))?;

        if let Some(word) = node.next.choose(&mut self.rng) {
            self.pos = word
                .next
                .unwrap_or_else(|| self.rng.random_range(..self.nodes.len()));
            return Some(word.word.of(self.text));
        }

        unreachable!()
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

pub static SENTENCE_ENDINGS: &'static [&'static str] = &[
    ".", "!", "?", ".\"", "!\"", "?\"", ".”", "!”", "?”", ".)", "?)", "!)",
];

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
            "for anyone else is there , too , which seems to suggest",
            "that art supplies something like an excitation , a memory",
            ", the pretext , the conventions of distance , which are",
            "functions of “ style ” consists of the work of art are",
            "defended as good or bad ? and that our response to something",
            "like an excitation , a phenomenon of commitment , judgment",
            "in most appraisals of serious novels , plays , and Tiffany ,",
            "in a work of art as a generic decision on the matter where",
            "he does , the notion of the whole",
        ]
        .into_iter()
        .flat_map(|line| line.split(' '))
        .collect::<Vec<_>>();

        let actual: Vec<&str> = generator.take(100).collect();

        assert_eq!(actual, expected);
    }
}
