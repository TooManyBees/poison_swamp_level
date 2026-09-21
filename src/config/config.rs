use super::{ParseError, load_config};
use crate::service::Address;
use http::StatusCode;
use http::header::HeaderName;
use log::LevelFilter;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::{ops::RangeInclusive, path::Path};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub classifier: Classifier,
    pub garbage: Garbage,
    pub server: Server,
    pub logging: Logging,
    pub metrics: Option<Metrics>,
}

impl Config {
    pub fn read_from_file<P: AsRef<Path>>(path: P) -> Result<Config, ParseError> {
        load_config(path)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Classifier {
    pub unwanted_asns: Vec<u32>,
    pub asns_db_path: Option<String>,
    pub robots_json_path: Option<String>,
    pub unwanted_agents: Vec<String>,
    pub trusted_ips: Vec<IpAddr>,
    pub trusted_paths: Vec<String>,
    pub trusted_agents: Vec<String>,
    pub trusted_decision_header: Option<HeaderName>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Garbage {
    pub source_files: Vec<String>,
    pub words_file: Option<String>,
    pub paragraphs: Paragraphs,
    pub links: Links,
    pub template_file: Option<String>,
    pub poisons: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, Eq)]
pub struct Links {
    pub(super) min_words: usize,
    pub(super) max_words: usize,
    pub(super) min_count: usize,
    pub(super) max_count: usize,
    pub separator: char,
    pub trailing_slash: bool,
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
            trailing_slash: false,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Server {
    pub listen: Address,
    pub mode: ServerMode,
    pub status_code_valid: StatusCode,
    pub status_code_spam: StatusCode,
}

impl Default for Server {
    fn default() -> Self {
        Server {
            listen: Address::TcpSocket(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::new(127, 0, 0, 1),
                4000,
            ))),
            mode: ServerMode::Preflight,
            status_code_valid: StatusCode::OK,
            status_code_spam: StatusCode::UNAUTHORIZED,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServerMode {
    Proxy,
    Preflight,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Logging {
    pub level: LevelFilter,
    pub target: LogTarget,
    pub color: bool,
    pub timestamps: bool,
    pub request_handler: bool,
}

impl Default for Logging {
    fn default() -> Logging {
        Logging {
            level: LevelFilter::Off,
            target: LogTarget::Stderr,
            color: false,
            timestamps: true,
            request_handler: false,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum LogTarget {
    Stdout,
    Stderr,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Metrics {
    pub listen: Address,
    pub persist_path: Option<String>,
}

impl Default for Metrics {
    fn default() -> Metrics {
        Metrics {
            listen: Address::TcpSocket(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::new(127, 0, 0, 1),
                4001,
            ))),
            persist_path: None,
        }
    }
}
