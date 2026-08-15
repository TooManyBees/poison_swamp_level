use super::generator::Corpus;
use crate::Config;
use rand::{Rng, RngExt, seq::IndexedRandom};
use rand_seeder::{Seeder, SipRng};
use std::collections::BTreeMap;
use std::fs;
use std::ops::RangeInclusive;
use upon::{Engine, Template, Value};

pub struct Garbage<'e> {
    corpus: Corpus,
    words: Vec<&'static str>,
    engine: Engine<'e>,
    template: Template<'e>,
    num_paragraphs: RangeInclusive<usize>,
    num_words: RangeInclusive<usize>,
    num_links: RangeInclusive<usize>,
    num_link_words: RangeInclusive<usize>,
    link_separator: char,
    poisons: Vec<String>,
}

impl<'e> Garbage<'e> {
    pub fn new(config: &Config) -> Self {
        let corpus = Corpus::from_files(&config.garbage.source_files).unwrap();
        let words: Vec<_> = {
            let s: &'static str = fs::read_to_string(config.garbage.words_file.as_ref().unwrap())
                .unwrap()
                .leak();
            s.lines().collect()
        };
        if words.len() < *config.garbage.links.num_words().end() {
            panic!(
                "Words list {} is shorter than config.garbage.links.max_words {}",
                config.garbage.words_file.as_ref().unwrap(),
                config.garbage.links.num_words().end()
            );
        }
        let template_str =
            fs::read_to_string(config.garbage.template_file.as_ref().unwrap()).unwrap();
        let engine = Engine::new();
        let template = engine.compile(template_str).unwrap();
        Garbage {
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
        }
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
                link_path.push(self.link_separator);
            }
            let num_words = rng.random_range(self.num_link_words.clone());
            let words: Vec<_> = self.words.sample(rng, num_words).copied().collect();
            for segment in &words {
                link_path.push_str(segment);
                link_path.push(self.link_separator);
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
        let links = if !is_poisoned {
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
