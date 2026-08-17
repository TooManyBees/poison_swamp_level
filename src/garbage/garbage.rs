use super::generator::Corpus;
use super::read_text::ParseError;
use crate::Config;
use rand::{Rng, RngExt, seq::IndexedRandom};
use rand_seeder::{Seeder, SipRng};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::ops::RangeInclusive;
use upon::{Engine, Template, Value};

pub struct Garbage {
    corpus: Corpus,
    words: Vec<&'static str>,
    engine: Engine<'static>,
    template: Template<'static>,
    num_paragraphs: RangeInclusive<usize>,
    num_words: RangeInclusive<usize>,
    num_links: RangeInclusive<usize>,
    num_link_words: RangeInclusive<usize>,
    link_separator: char,
    poisons: Vec<String>,
}

impl Garbage {
    pub fn new(config: &Config) -> Result<Self, GarbageError> {
        let corpus = Corpus::from_files(&config.garbage.source_files)?;

        let words = {
            let words_file = config
                .garbage
                .words_file
                .as_ref()
                .ok_or(GarbageError::WordsFileMissing)?;
            let s = fs::read_to_string(words_file)?.leak();
            let lines: Vec<_> = s.lines().collect();
            let min_words_needed = *config.garbage.links.num_words().end();
            if lines.len() < min_words_needed {
                return Err(GarbageError::WordsListTooShort(
                    words_file.to_string(),
                    min_words_needed,
                ));
            }
            lines
        };

        let (engine, template) = {
            let template_path = config
                .garbage
                .template_file
                .as_ref()
                .ok_or(GarbageError::TemplateFileMissing)?;
            let template_str = fs::read_to_string(template_path)?.leak();
            let engine = Engine::new();
            let template = engine.compile(&*template_str)?;
            (engine, template)
        };

        Ok(Garbage {
            corpus,
            words,
            engine,
            template,
            num_paragraphs: config.garbage.paragraphs.count(),
            num_words: config.garbage.paragraphs.num_words(),
            num_links: config.garbage.links.count(),
            num_link_words: config.garbage.links.num_words(),
            link_separator: config.garbage.links.separator,
            poisons: config.garbage.poisons.clone(),
        })
    }

    fn generate_links<R: Rng>(&self, path: &str, rng: &mut R) -> Value {
        let num_links = rng.random_range(self.num_links.clone());
        let mut links = Vec::with_capacity(num_links);
        for _ in 0..num_links {
            let mut link_path = if path.ends_with('/') {
                path.to_string()
            } else {
                format!("{path}/")
            };
            if let Some(p) = self.poisons.choose(rng) {
                link_path.push_str(p);
            }
            let num_words = rng.random_range(self.num_link_words.clone());
            let words: Vec<_> = self.words.sample(rng, num_words).copied().collect();
            for segment in &words {
                link_path.push(self.link_separator);
                link_path.push_str(segment);
            }
            let text = words.join(" ");
            links.push(Value::Map(BTreeMap::from([
                ("path".into(), Value::String(link_path)),
                ("text".into(), Value::String(text)),
            ])))
        }
        Value::List(links)
    }

    pub fn render(&self, path: &str) -> String {
        let mut rng: SipRng = Seeder::from(path).into_rng();
        let mut generator = self.corpus.generator(&mut rng);

        let paragraphs: Vec<_> = generator
            .paragraphs(&self.num_paragraphs, &self.num_words)
            .map(Value::String)
            .collect();

        let is_poisoned = self.poisons.iter().any(|p| path.contains(p));
        let links = if !self.poisons.is_empty() && !is_poisoned {
            self.generate_links(path, generator.rng())
        } else {
            Value::None
        };

        let data = Value::Map(BTreeMap::from([
            ("title".into(), Value::String("garbage".to_string())),
            ("paragraphs".into(), Value::List(paragraphs)),
            ("links".into(), links),
        ]));
        let renderer = self.template.render_from(&self.engine, &data);
        renderer.to_string().unwrap()
    }
}

impl fmt::Debug for Garbage {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Garbage")
            .field("corpus", &format_args!("Corpus {{ .. }}"))
            .field("words", &format_args!("[ .. ]"))
            .field("engine", &format_args!("Engine {{ .. }}"))
            .field("template", &format_args!("Template {{ .. }}"))
            .field("num_paragraphs", &self.num_paragraphs)
            .field("num_words", &self.num_words)
            .field("num_links", &self.num_links)
            .field("num_link_words", &self.num_link_words)
            .field("link_separator", &self.link_separator)
            .field("poisons", &self.poisons)
            .finish()
    }
}

#[derive(Debug)]
pub enum GarbageError {
    Io(std::io::Error),
    CorpusEmpty,
    WordsFileMissing,
    WordsListTooShort(String, usize),
    TemplateFileMissing,
    Template(upon::Error),
}

impl From<std::io::Error> for GarbageError {
    fn from(e: std::io::Error) -> GarbageError {
        GarbageError::Io(e)
    }
}

impl From<upon::Error> for GarbageError {
    fn from(e: upon::Error) -> GarbageError {
        GarbageError::Template(e)
    }
}

impl From<ParseError> for GarbageError {
    fn from(e: ParseError) -> GarbageError {
        match e {
            ParseError::Io(e) => GarbageError::Io(e),
            ParseError::NoContent => GarbageError::CorpusEmpty,
        }
    }
}
