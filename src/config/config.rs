use serde::Deserialize;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::{error::Error, fs::File, ops::RangeInclusive, path::Path};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub poisons: Vec<String>,
    pub classifier: Classifier,
    pub garbage: Garbage,
    pub server: Server,
}

impl Config {
    pub fn read_from_file<P: AsRef<Path>>(path: P) -> Result<Config, Box<dyn Error>> {
        let file = File::open(path)?;
        let config = serde_json::from_reader(file)?;
        Ok(config)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Classifier {
    pub unwanted_asns: Vec<u32>,
    pub asns_db_path: Option<String>,
    pub trusted_decision_header: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Garbage {
    pub source_files: Vec<String>,
    pub words_file: Option<String>,
    pub paragraphs: Paragraphs,
    pub links: Links,
    pub template_file: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Paragraphs {
    min_words: usize,
    max_words: usize,
    min_count: usize,
    max_count: usize,
}

impl Paragraphs {
    pub fn num_words(&self) -> RangeInclusive<usize> {
        self.min_words..=self.max_words
    }

    pub fn count(&self) -> RangeInclusive<usize> {
        self.min_count..=self.max_count
    }
}

impl Default for Paragraphs {
    fn default() -> Self {
        Paragraphs {
            min_words: 16,
            max_words: 32,
            min_count: 4,
            max_count: 6,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Links {
    min_words: usize,
    max_words: usize,
    min_count: usize,
    max_count: usize,
    pub separator: char,
}

impl Links {
    pub fn num_words(&self) -> RangeInclusive<usize> {
        self.min_words..=self.max_words
    }

    pub fn count(&self) -> RangeInclusive<usize> {
        self.min_count..=self.max_count
    }
}

impl Default for Links {
    fn default() -> Self {
        Links {
            min_words: 2,
            max_words: 4,
            min_count: 2,
            max_count: 5,
            separator: '-',
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Server {
    pub interface: SocketAddr,
}

impl Default for Server {
    fn default() -> Self {
        Server {
            interface: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 4000)),
        }
    }
}
