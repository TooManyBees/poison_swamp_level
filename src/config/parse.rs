use super::config::{Classifier, Config, Garbage, Links, Paragraphs, Server, ServerMode};
use http::status::{InvalidStatusCode, StatusCode};
use kdl::{KdlDocument, KdlError, KdlNode};
use std::fmt::Debug;
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
            "classifier" => {
                config.classifier = node.parse_classifier()?;
            }
            "garbage" => {
                config.garbage = node.parse_garbage()?;
            }
            _ => {}
        }
    }

    Ok(config)
}

trait Parseable {
    fn parse_server(&self) -> Result<Server, ParseError>;

    fn parse_classifier(&self) -> Result<Classifier, ParseError>;

    fn parse_garbage(&self) -> Result<Garbage, ParseError>;

    fn one_string_arg(&self) -> Result<&str, ParseError>;

    fn int_prop(&self, name: &str) -> Result<Option<i128>, ParseError>;

    fn string_seq(&self) -> Result<Vec<String>, ParseError>;

    fn int_seq<I: Debug + TryFrom<i128>>(&self) -> Result<Vec<I>, ParseError>;
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
                    if let Some(status) = child.int_prop("valid")? {
                        server.status_code_valid = StatusCode::from_u16(u16::try_from(status)?)?;
                    }
                    if let Some(status) = child.int_prop("spam")? {
                        server.status_code_spam = StatusCode::from_u16(u16::try_from(status)?)?;
                    }
                }
                _ => {}
            }
        }

        Ok(server)
    }

    fn parse_classifier(&self) -> Result<Classifier, ParseError> {
        let mut classifier = Classifier::default();

        for child in self.iter_children() {
            match child.name().value() {
                "trusted-decision-header" => {
                    classifier.trusted_decision_header = Some(child.one_string_arg()?.to_string());
                }
                "user-agents" => {
                    for child in child.iter_children() {
                        match child.name().value() {
                            "robots-json-path" => {
                                classifier.robots_json_path =
                                    Some(child.one_string_arg()?.to_string())
                            }
                            "unwanted" => classifier.unwanted_agents = child.string_seq()?,
                            "trusted" => classifier.trusted_agents = child.string_seq()?,
                            _ => {}
                        }
                    }
                }
                "trusted-paths" => {
                    classifier.trusted_paths = child.string_seq()?;
                }
                "asns" => {
                    for child in child.iter_children() {
                        match child.name().value() {
                            "database" => {
                                classifier.asns_db_path = Some(child.one_string_arg()?.to_string())
                            }
                            "unwanted" => classifier.unwanted_asns = child.int_seq()?,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(classifier)
    }

    fn parse_garbage(&self) -> Result<Garbage, ParseError> {
        let mut garbage = Garbage::default();

        for child in self.iter_children() {
            match child.name().value() {
                "corpus-files" => {
                    garbage.source_files = child.string_seq()?;
                }
                "words-file" => {
                    garbage.words_file = Some(child.one_string_arg()?.to_string());
                }
                "template-file" => {
                    garbage.template_file = Some(child.one_string_arg()?.to_string());
                }
                "poisons" => {
                    // garbage.poisons = child.string_seq()?;
                }
                "paragraphs" => {}
                "links" => {}
                _ => {}
            }
        }

        Ok(garbage)
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

    fn int_prop(&self, name: &str) -> Result<Option<i128>, ParseError> {
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

    fn string_seq(&self) -> Result<Vec<String>, ParseError> {
        let mut seq = Vec::new();
        for entry in self.iter() {
            if let Some(_name) = entry.name() {
                return Err(ParseError::Invalid {
                    field: self.name().value().to_string(),
                    message: format!("{} must not have named properties", self.name().value()),
                });
            }
            match entry.value().as_string() {
                Some(s) => seq.push(s.to_string()),
                None => {
                    return Err(ParseError::Invalid {
                        field: self.name().value().to_string(),
                        message: format!("{} must be a sequence of strings", self.name().value()),
                    });
                }
            }
        }
        seq.shrink_to_fit();
        Ok(seq)
    }

    fn int_seq<I: Debug + TryFrom<i128>>(&self) -> Result<Vec<I>, ParseError> {
        let mut seq = Vec::new();
        for entry in self.iter() {
            if let Some(_name) = entry.name() {
                return Err(ParseError::Invalid {
                    field: self.name().value().to_string(),
                    message: format!("{} must not have named properties", self.name().value()),
                });
            }
            match entry.value().as_integer() {
                Some(i) => {
                    let converted = I::try_from(i).map_err(|_| ParseError::Invalid {
                        field: self.name().value().to_string(),
                        message: format!("it doesn't work, babe"),
                    })?;
                    seq.push(converted);
                }
                None => {
                    return Err(ParseError::Invalid {
                        field: self.name().value().to_string(),
                        message: format!(
                            "{} must be a sequence of {} integers",
                            self.name().value(),
                            std::any::type_name::<I>()
                        ),
                    });
                }
            }
        }
        seq.shrink_to_fit();
        Ok(seq)
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
