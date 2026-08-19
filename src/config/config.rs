use super::{ParseError, load_config};
use http::StatusCode;
use log::LevelFilter;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::{ops::RangeInclusive, path::Path};

#[derive(Debug, Default)]
pub struct Config {
    pub classifier: Classifier,
    pub garbage: Garbage,
    pub server: Server,
    pub logging: Logging,
}

impl Config {
    pub fn read_from_file<P: AsRef<Path>>(path: P) -> Result<Config, ParseError> {
        load_config(path)
    }
}

#[derive(Debug, Default)]
pub struct Classifier {
    pub unwanted_asns: Vec<u32>,
    pub asns_db_path: Option<String>,
    pub robots_json_path: Option<String>,
    pub unwanted_agents: Vec<String>,
    pub trusted_ips: Vec<IpAddr>,
    pub trusted_paths: Vec<String>,
    pub trusted_agents: Vec<String>,
    pub trusted_decision_header: Option<String>,
}

#[derive(Debug, Default)]
pub struct Garbage {
    pub source_files: Vec<String>,
    pub words_file: Option<String>,
    pub paragraphs: Paragraphs,
    pub links: Links,
    pub template_file: Option<String>,
    pub poisons: Vec<String>,
}

#[derive(Debug)]
pub struct Paragraphs {
    pub(super) min_words: usize,
    pub(super) max_words: usize,
    pub(super) min_count: usize,
    pub(super) max_count: usize,
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

#[derive(Debug)]
pub struct Links {
    pub(super) min_words: usize,
    pub(super) max_words: usize,
    pub(super) min_count: usize,
    pub(super) max_count: usize,
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

#[derive(Debug)]
pub struct Server {
    pub listen: SocketAddr,
    pub mode: ServerMode,
    pub status_code_valid: StatusCode,
    pub status_code_spam: StatusCode,
}

impl Default for Server {
    fn default() -> Self {
        Server {
            listen: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 4000)),
            mode: ServerMode::Preflight,
            status_code_valid: StatusCode::OK,
            status_code_spam: StatusCode::UNAUTHORIZED,
        }
    }
}

#[derive(Debug)]
pub enum ServerMode {
    Proxy,
    Preflight,
}

#[derive(Debug)]
pub struct Logging {
    pub level: LevelFilter,
    pub request_handler: bool,
}

impl Default for Logging {
    fn default() -> Logging {
        Logging {
            level: LevelFilter::Off,
            request_handler: false,
        }
    }
}
