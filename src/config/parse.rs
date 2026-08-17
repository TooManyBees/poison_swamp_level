use super::config::{Classifier, Config, Garbage, Server, ServerMode};
use http::status::{InvalidStatusCode, StatusCode};
use kdl::{KdlDocument, KdlEntry, KdlError, KdlNode};
use std::{fmt::Debug, fs, net::AddrParseError, num::TryFromIntError, path::Path};

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

    fn int_prop<I: Debug + TryFrom<i128>>(&self, name: &str) -> Result<Option<I>, ParseError>;

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
                    if let Some(status) = child.int_prop::<u16>("valid")? {
                        server.status_code_valid = StatusCode::from_u16(status)?;
                    }
                    if let Some(status) = child.int_prop::<u16>("spam")? {
                        server.status_code_spam = StatusCode::from_u16(status)?;
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
                    garbage.poisons = child.string_seq()?;
                }
                "paragraphs" => {
                    if let Some(n) = child.int_prop::<usize>("min")? {
                        garbage.paragraphs.min_count = n;
                    }
                    if let Some(n) = child.int_prop::<usize>("max")? {
                        garbage.paragraphs.max_count = n;
                    }
                    for child in child.iter_children() {
                        match child.name().value() {
                            "words" => {
                                if let Some(n) = child.int_prop::<usize>("min")? {
                                    garbage.paragraphs.min_words = n;
                                }
                                if let Some(n) = child.int_prop::<usize>("max")? {
                                    garbage.paragraphs.max_words = n;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "links" => {
                    if let Some(n) = child.int_prop::<usize>("min")? {
                        garbage.links.min_count = n;
                    }
                    if let Some(n) = child.int_prop::<usize>("max")? {
                        garbage.links.max_count = n;
                    }
                    for child in child.iter_children() {
                        match child.name().value() {
                            "words" => {
                                if let Some(n) = child.int_prop::<usize>("min")? {
                                    garbage.links.min_words = n;
                                }
                                if let Some(n) = child.int_prop::<usize>("max")? {
                                    garbage.links.max_words = n;
                                }
                            }
                            "separator" => {
                                garbage.links.separator =
                                    child.one_string_arg()?.chars().nth(0).unwrap();
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(garbage)
    }

    fn one_string_arg(&self) -> Result<&str, ParseError> {
        let entry = self
            .entry(0)
            .ok_or_else(|| ParseError::from_node(self, "must have one string argument".into()))?;
        match entry.value().as_string() {
            Some("") => Err(ParseError::from_entry(entry, "must not be empty".into())),
            Some(arg) => Ok(arg),
            None => Err(ParseError::from_entry(entry, "must be a string".into())),
        }
    }

    fn int_prop<I: Debug + TryFrom<i128>>(&self, name: &str) -> Result<Option<I>, ParseError> {
        self.iter()
            .find(|e| e.name().map(|n| n.value() == name).unwrap_or(false))
            .map(|entry| {
                entry
                    .value()
                    .as_integer()
                    .and_then(|n| n.try_into().ok())
                    .ok_or_else(|| {
                        ParseError::from_entry(
                            entry,
                            format!("must be a {} integer", std::any::type_name::<I>()),
                        )
                    })
            })
            .transpose()
    }

    fn string_seq(&self) -> Result<Vec<String>, ParseError> {
        let mut seq = Vec::new();
        for entry in self.iter() {
            if entry.name().is_some() {
                return Err(ParseError::from_entry(
                    entry,
                    "must not be a named property".into(),
                ));
            }
            match entry.value().as_string() {
                Some(s) => seq.push(s.to_string()),
                None => {
                    return Err(ParseError::from_entry(entry, "must be a string".into()));
                }
            }
        }
        seq.shrink_to_fit();
        Ok(seq)
    }

    fn int_seq<I: Debug + TryFrom<i128>>(&self) -> Result<Vec<I>, ParseError> {
        let mut seq = Vec::new();
        for entry in self.iter() {
            if entry.name().is_some() {
                return Err(ParseError::from_entry(
                    entry,
                    "must not be a namedj property".into(),
                ));
            }
            let int = entry
                .value()
                .as_integer()
                .and_then(|v| v.try_into().ok())
                .ok_or_else(|| {
                    ParseError::from_entry(
                        entry,
                        format!("must be a {} integer", std::any::type_name::<I>()),
                    )
                })?;
            seq.push(int);
        }
        seq.shrink_to_fit();
        Ok(seq)
    }
}

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Kdl(KdlError),
    InvalidNode {
        field: String,
        offset: usize,
        message: String,
    },
    InvalidEntry {
        offset: usize,
        message: String,
    },
    InvalidSocketAddr(AddrParseError),
    Int(TryFromIntError),
    StatusCode,
}

impl ParseError {
    fn from_node(node: &KdlNode, message: String) -> ParseError {
        ParseError::InvalidNode {
            field: node.name().value().to_string(),
            offset: node.span().offset(),
            message,
        }
    }

    fn from_entry(entry: &KdlEntry, message: String) -> ParseError {
        ParseError::InvalidEntry {
            offset: entry.span().offset(),
            message,
        }
    }
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
    fn from(_: InvalidStatusCode) -> ParseError {
        ParseError::StatusCode
    }
}

#[cfg(test)]
mod test {
    use super::{ParseError, Parseable};
    use kdl::KdlDocument;
    use std::assert_matches;

    #[test]
    fn int_seq() {
        let doc: KdlDocument = "numbers 1 2 3 4".parse().unwrap();
        let numbers = doc.nodes().first().unwrap();
        assert_matches!(numbers.int_seq::<u8>().as_deref(), Ok(&[1u8, 2, 3, 4]));
    }

    #[test]
    fn int_seq_parses_empty_node() {
        let doc: KdlDocument = "numbers".parse().unwrap();
        let numbers = doc.nodes().first().unwrap();
        assert_matches!(numbers.int_seq::<u8>().as_deref(), Ok(&[]));
    }

    #[test]
    fn int_seq_rejects_named_props() {
        let doc: KdlDocument = "numbers 1 2 3 four=4".parse().unwrap();
        let numbers = doc.nodes().first().unwrap();
        assert_matches!(
            numbers.int_seq::<u8>(),
            Err(ParseError::InvalidEntry { .. })
        );
    }

    #[test]
    fn int_seq_rejects_string_props() {
        let doc: KdlDocument = "numbers 1 two three 4".parse().unwrap();
        let numbers = doc.nodes().first().unwrap();
        assert_matches!(
            numbers.int_seq::<u8>(),
            Err(ParseError::InvalidEntry { .. })
        );
    }

    #[test]
    fn int_seq_rejects_incompatible_numbers() {
        let doc: KdlDocument = "numbers 254 255 256 257".parse().unwrap();
        let numbers = doc.nodes().first().unwrap();
        assert_matches!(
            numbers.int_seq::<u8>(),
            Err(ParseError::InvalidEntry { .. })
        );
    }
}
