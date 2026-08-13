mod generator;
mod read_text;

use crate::Config;
pub use generator::Corpus;
use generator::{JOIN_AFTER, JOIN_BEFORE, State, Substring, is_ending};
use rand::seq::IteratorRandom;
use rand_seeder::{Seeder, SipRng};
use read_text::{ParseError, read, read_from_files, read_from_strings};
use std::collections::BTreeMap;
use std::fs;
use std::ops::RangeInclusive;
use upon::{Engine, Template, Value};

pub struct Garbage<'e> {
    corpus: Corpus,
    engine: Engine<'e>,
    template: Template<'e>,
    num_paragraphs: RangeInclusive<usize>,
    num_words: RangeInclusive<usize>,
}

impl<'e> Garbage<'e> {
    pub fn new(config: &Config) -> Self {
        let corpus_paths = vec!["./susan.sontag.notes.on.camp.txt"];
        let template_path = "garbage.html";
        let corpus = Corpus::from_files(&corpus_paths).unwrap();
        let template_str = fs::read_to_string(template_path).unwrap();
        let engine = Engine::new();
        let template = engine.compile(template_str).unwrap();
        Garbage {
            corpus,
            engine,
            template,
            num_paragraphs: config.garbage.paragraphs.count(),
            num_words: config.garbage.paragraphs.num_words(),
        }
    }

    pub fn render(&self, path: &str) -> String {
        let mut rng: SipRng = Seeder::from(path).into_rng();
        let num_paragraphs = self.num_paragraphs.clone().choose(&mut rng).unwrap();
        let mut generator = self.corpus.generator(&mut rng);

        let mut paragraphs = Vec::with_capacity(num_paragraphs);
        for _ in 0..num_paragraphs {
            paragraphs.push(generator.generate(self.num_words.clone()));
        }

        let data = Value::Map(BTreeMap::from([
            ("title".into(), "garbage".into()),
            ("paragraphs".into(), paragraphs.into()),
        ]));
        let renderer = self.template.render_from(&self.engine, &data);
        renderer.to_string().unwrap()
    }
}
