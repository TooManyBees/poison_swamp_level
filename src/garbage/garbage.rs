use super::generator::Corpus;
use crate::Config;
use rand_seeder::{Seeder, SipRng};
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
    num_links: RangeInclusive<usize>,
}

impl<'e> Garbage<'e> {
    pub fn new(config: &Config) -> Self {
        let corpus = Corpus::from_files(&config.garbage.source_files).unwrap();
        let template_str =
            fs::read_to_string(config.garbage.template_file.as_ref().unwrap()).unwrap();
        let engine = Engine::new();
        let template = engine.compile(template_str).unwrap();
        Garbage {
            corpus,
            engine,
            template,
            num_paragraphs: config.garbage.paragraphs.count(),
            num_words: config.garbage.paragraphs.num_words(),
            num_links: config.garbage.links.count(),
        }
    }

    pub fn render(&self, path: &str) -> String {
        let mut rng: SipRng = Seeder::from(path).into_rng();
        let mut generator = self.corpus.generator(&mut rng);

        let paragraphs: Vec<_> = generator
            .paragraphs(&self.num_paragraphs, &self.num_words)
            .map(Value::String)
            .collect();

        let links = if true {
            let num_links = 3; // fixme
            let mut links = Vec::with_capacity(num_links); // fixme
            for _ in 0..num_links {
                let mut path = String::new();
                for segment in generator.words(3..=5) {
                    path.push('/');
                    path.push_str(segment);
                }
                let text = generator.generate(3..=5);
                links.push(Value::Map(BTreeMap::from([
                    ("path".into(), path.into()),
                    ("text".into(), text.into()),
                ])))
            }
            Value::List(links)
        } else {
            Value::None
        };

        let data = Value::Map(BTreeMap::from([
            ("title".into(), Value::String("garbage")),
            ("paragraphs".into(), Value::List(paragraphs)),
            ("links".into(), links),
        ]));
        let renderer = self.template.render_from(&self.engine, &data);
        renderer.to_string().unwrap()
    }
}
