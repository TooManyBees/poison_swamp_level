use super::config::{Classifier, Config, Garbage, Links, Paragraphs, Server, ServerMode};
use http::{StatusCode, status::InvalidStatusCode};
use kdl::{KdlDocument, KdlError, KdlNode};
use std::fs;
use std::net::AddrParseError;
use std::num::TryFromIntError;
use std::path::Path;

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, ParseError> {
    let doc = load_file(path)?;
    let config = parse_doc(doc)?;
    Ok(config)
}

fn load_file<P: AsRef<Path>>(path: P) -> Result<KdlDocument, ParseError> {
    let s = fs::read_to_string(path)?;
    let doc = s.parse::<KdlDocument>().unwrap();
    Ok(doc)
}

fn parse_doc(doc: KdlDocument) -> Result<Config, ParseError> {
    let mut config = Config::default();

    for node in doc.nodes() {
        match node.name().value() {
            "server" => {
                config.server = node.parse_server()?;
            }
            _ => {}
        }
    }

    Ok(config)
}

trait Parseable {
    fn parse_server(&self) -> Result<Server, ParseError>;

    fn one_string_arg(&self) -> Result<&str, ParseError>;

    fn numeric_prop(&self, name: &str) -> Result<Option<i128>, ParseError>;

    // fn string_seq(&self) -> Result<Vec<String>, ParseError>;
}

impl Parseable for KdlNode {
    fn parse_server(&self) -> Result<Server, ParseError> {
        let mut server = Server::default();

        match self.one_string_arg()? {
            "proxy" => server.mode = ServerMode::Proxy,
            "preflight" => server.mode = ServerMode::Preflight,
            m => panic!("never heard of a {m} mode"),
        }

        for child in self.iter_children() {
            match child.name().value() {
                "listen" => {
                    server.listen = child.one_string_arg()?.parse()?;
                }
                "status-codes" => {
                    if let Some(status) = child.numeric_prop("valid")? {
                        server.status_code_valid = StatusCode::from_u16(u16::try_from(status)?)?;
                    }
                    if let Some(status) = child.numeric_prop("spam")? {
                        server.status_code_spam = StatusCode::from_u16(u16::try_from(status)?)?;
                    }
                }
                _ => {}
            }
        }

        Ok(server)
    }

    fn one_string_arg(&self) -> Result<&str, ParseError> {
        self.entry(0)
            .map(|e| e.value())
            .ok_or_else(|| ParseError::Invalid {
                field: self.name().value().to_string(),
                message: format!(
                    "field {} at {} must have one argument",
                    self.name().value(),
                    self.span().offset()
                ),
            })?
            .as_string()
            .ok_or_else(|| ParseError::Invalid {
                field: self.name().value().to_string(),
                message: format!(
                    "field {} at {} must have a string argument",
                    self.name().value(),
                    self.span().offset()
                ),
            })
    }

    fn numeric_prop(&self, name: &str) -> Result<Option<i128>, ParseError> {
        self.get(name)
            .map(|prop| {
                prop.as_integer().ok_or_else(|| ParseError::Invalid {
                    field: self.name().value().to_string(),
                    message: format!(
                        "property {name} at {} must be an integer",
                        self.span().offset(),
                    ),
                })
            })
            .transpose()
    }
}

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Kdl(KdlError),
    Invalid { field: String, message: String },
    InvalidSocketAddr(AddrParseError),
    Int(TryFromIntError),
    StatusCode(InvalidStatusCode),
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> ParseError {
        ParseError::Io(e)
    }
}

impl From<KdlError> for ParseError {
    fn from(e: KdlError) -> ParseError {
        ParseError::Kdl(e)
    }
}

impl From<AddrParseError> for ParseError {
    fn from(e: AddrParseError) -> ParseError {
        ParseError::InvalidSocketAddr(e)
    }
}

impl From<TryFromIntError> for ParseError {
    fn from(e: TryFromIntError) -> ParseError {
        ParseError::Int(e)
    }
}

impl From<InvalidStatusCode> for ParseError {
    fn from(e: InvalidStatusCode) -> ParseError {
        ParseError::StatusCode(e)
    }
}
