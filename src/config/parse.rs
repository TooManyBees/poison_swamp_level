use super::ParseError;
use super::config::{
    Classifier, Config, Garbage, Links, LogTarget, Logging, Metrics, Server, ServerMode,
};
use http::header::HeaderName;
use http::status::StatusCode;
use kdl::{KdlDocument, KdlEntry, KdlNode};
use log::LevelFilter;
use std::{fmt, fs, ops::Deref, path::Path, str::FromStr};

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, ParseError> {
    let source =
        fs::read_to_string(&path).map_err(|e| ParseError::Io(path.as_ref().to_path_buf(), e))?;
    let doc = source.parse::<KdlDocument>()?;
    match parse_doc(doc) {
        Ok(config) => Ok(config),
        Err(mut e) => {
            e.set_source_code(source);
            Err(e)
        }
    }
}

fn parse_doc(doc: KdlDocument) -> Result<Config, ParseError> {
    let mut config = Config::default();

    for node in doc.nodes() {
        match node.name().value() {
            "server" => {
                config.server = parse_server(node)?;
            }
            "classifier" => {
                config.classifier = parse_classifier(node)?;
            }
            "garbage" => {
                config.garbage = parse_garbage(node)?;
            }
            "logging" => {
                config.logging = parse_logging(node)?;
            }
            "metrics" => {
                config.metrics = Some(parse_metrics(node)?);
            }
            _ => {}
        }
    }

    Ok(config)
}

fn parse_server(node: &KdlNode) -> Result<Server, ParseError> {
    let mut server = Server::default();

    let mode_entry = node.one_string_entry()?;
    match mode_entry.as_ref() {
        "proxy" => server.mode = ServerMode::Proxy,
        "preflight" => server.mode = ServerMode::Preflight,
        _ => {
            return Err(ParseError::from_entry(
                &mode_entry,
                "unsupported server mode".into(),
            ));
        }
    }

    for child in node.iter_children() {
        match child.name().value() {
            "listen" => {
                let entry = child.one_string_entry()?;
                server.listen = entry
                    .as_ref()
                    .parse()
                    .map_err(|_| ParseError::from_entry(&entry, "invalid socket address".into()))?;
            }
            "status-codes" => {
                if let Some((entry, status)) = child.int_prop_with_entry::<u16>("valid")? {
                    server.status_code_valid = StatusCode::from_u16(status).map_err(|_| {
                        ParseError::from_entry(entry, "invalid HTTP status code".into())
                    })?;
                }
                if let Some((entry, status)) = child.int_prop_with_entry::<u16>("spam")? {
                    server.status_code_spam = StatusCode::from_u16(status).map_err(|_| {
                        ParseError::from_entry(entry, "invalid HTTP status code".into())
                    })?;
                }
            }
            _ => {}
        }
    }

    Ok(server)
}

fn parse_classifier(node: &KdlNode) -> Result<Classifier, ParseError> {
    let mut classifier = Classifier::default();

    for child in node.iter_children() {
        match child.name().value() {
            "trusted-decision-header" => {
                let entry = child.one_string_entry()?;
                match HeaderName::from_bytes(entry.as_ref().as_bytes()) {
                    Ok(header) => classifier.trusted_decision_header = Some(header),
                    Err(e) => return Err(ParseError::from_entry(&entry, format!("{e}"))),
                }
            }
            "user-agents" => {
                for child in child.iter_children() {
                    match child.name().value() {
                        "robots-json-path" => {
                            classifier.robots_json_path = Some(child.one_string_arg()?)
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
                        "database" => classifier.asns_db_path = Some(child.one_string_arg()?),
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

fn parse_garbage(node: &KdlNode) -> Result<Garbage, ParseError> {
    let mut garbage = Garbage::default();

    for child in node.iter_children() {
        match child.name().value() {
            "corpus-files" => {
                garbage.source_files = child.string_seq()?;
            }
            "words-file" => {
                garbage.words_file = Some(child.one_string_arg()?);
            }
            "template-file" => {
                garbage.template_file = Some(child.one_string_arg()?);
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
                garbage.links = parse_garbage_links(child)?;
            }
            _ => {}
        }
    }

    Ok(garbage)
}

fn parse_garbage_links(node: &KdlNode) -> Result<Links, ParseError> {
    let mut links = Links::default();

    if let Some(n) = node.int_prop::<usize>("min")? {
        links.min_count = n;
    }
    if let Some(n) = node.int_prop::<usize>("max")? {
        links.max_count = n;
    }
    for child in node.iter_children() {
        match child.name().value() {
            "words" => {
                if let Some(n) = child.int_prop::<usize>("min")? {
                    links.min_words = n;
                }
                if let Some(n) = child.int_prop::<usize>("max")? {
                    links.max_words = n;
                }
            }
            "separator" => {
                links.separator = child
                    .one_string_arg()?
                    .chars()
                    .nth(0)
                    .expect("string is already tested to not be empty");
            }
            "trailing-slash" => links.trailing_slash = child.one_booleanish_entry()?,
            _ => {}
        }
    }

    Ok(links)
}

fn parse_logging(node: &KdlNode) -> Result<Logging, ParseError> {
    let mut logging = Logging::default();

    for child in node.iter_children() {
        match child.name().value() {
            "level" => {
                let entry = child.one_string_entry()?;
                logging.level = match LevelFilter::from_str(entry.as_ref()) {
                    Ok(level) => level,
                    Err(_) => {
                        return Err(ParseError::from_entry(&entry, "invalid log level".into()));
                    }
                };
            }
            "target" => {
                let entry = child.one_string_entry()?;
                logging.target = match entry.as_ref() {
                    "stdout" => LogTarget::Stdout,
                    "stderr" => LogTarget::Stderr,
                    _ => {
                        return Err(ParseError::from_entry(&entry, "invalid log target".into()));
                    }
                };
            }
            "color" => {
                logging.color = child.one_booleanish_entry()?;
            }
            "request-handler" => {
                logging.request_handler = child.one_booleanish_entry()?;
            }
            _ => {}
        }
    }

    Ok(logging)
}

fn parse_metrics(node: &KdlNode) -> Result<Metrics, ParseError> {
    let mut metrics = Metrics::default();

    for child in node.iter_children() {
        match child.name().value() {
            "listen" => {
                let entry = child.one_string_entry()?;
                metrics.listen = entry
                    .as_ref()
                    .parse()
                    .map_err(|_| ParseError::from_entry(&entry, "invalid socket address".into()))?;
            }
            "persist-path" => {
                metrics.persist_path = Some(child.one_string_arg()?);
            }
            _ => {}
        }
    }

    Ok(metrics)
}

trait Parseable {
    fn one_booleanish_entry(&self) -> Result<bool, ParseError>;

    fn one_string_entry<'a>(&'a self) -> Result<StringEntry<'a>, ParseError>;

    fn one_string_arg(&self) -> Result<String, ParseError>;

    fn int_prop<I: fmt::Debug + TryFrom<i128>>(&self, name: &str) -> Result<Option<I>, ParseError> {
        let res = self.int_prop_with_entry(name)?;
        Ok(res.map(|(_, i)| i))
    }

    fn int_prop_with_entry<I: fmt::Debug + TryFrom<i128>>(
        &self,
        name: &str,
    ) -> Result<Option<(&KdlEntry, I)>, ParseError>;

    fn string_seq(&self) -> Result<Vec<String>, ParseError>;

    fn int_seq<I: fmt::Debug + TryFrom<i128>>(&self) -> Result<Vec<I>, ParseError>;
}

struct StringEntry<'a>(&'a KdlEntry, &'a str);
impl<'a> Deref for StringEntry<'a> {
    type Target = KdlEntry;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}
impl<'a> AsRef<str> for StringEntry<'a> {
    fn as_ref(&self) -> &str {
        self.1
    }
}

impl Parseable for KdlNode {
    fn one_booleanish_entry(&self) -> Result<bool, ParseError> {
        let entry = self
            .entry(0)
            .ok_or_else(|| ParseError::from_node_name(self, "missing argument".into()))?;

        if let Some(b) = entry.value().as_bool() {
            return Ok(b);
        }
        match entry.value().as_string() {
            Some("on") => return Ok(true),
            Some("true") => return Ok(true),
            Some("off") => return Ok(false),
            Some("false") => return Ok(false),
            _ => Err(ParseError::from_entry(
                entry,
                "must have one boolean(ish) argument".into(),
            )),
        }
    }

    fn one_string_entry<'a>(&'a self) -> Result<StringEntry<'a>, ParseError> {
        let entry = self
            .entry(0)
            .ok_or_else(|| ParseError::from_node_name(self, "missing argument".into()))?;

        if let Some(string) = entry.value().as_string() {
            Ok(StringEntry(entry, string))
        } else {
            Err(ParseError::from_entry(
                entry,
                format!(
                    "{} block must have one string argument",
                    self.name().value().to_string()
                ),
            ))
        }
    }

    fn one_string_arg(&self) -> Result<String, ParseError> {
        let entry = self.one_string_entry()?;
        match entry.as_ref() {
            "" => Err(ParseError::from_entry(&entry, "must not be empty".into())),
            arg => Ok(arg.to_string()),
        }
    }

    fn int_prop_with_entry<I: fmt::Debug + TryFrom<i128>>(
        &self,
        name: &str,
    ) -> Result<Option<(&KdlEntry, I)>, ParseError> {
        self.iter()
            .find(|e| e.name().map(|n| n.value() == name).unwrap_or(false))
            .map(|entry| {
                entry
                    .value()
                    .as_integer()
                    .and_then(|n| n.try_into().ok())
                    .map(|n| (entry, n))
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

    fn int_seq<I: fmt::Debug + TryFrom<i128>>(&self) -> Result<Vec<I>, ParseError> {
        let mut seq = Vec::new();
        for entry in self.iter() {
            if entry.name().is_some() {
                return Err(ParseError::from_entry(
                    entry,
                    "must not be a named property".into(),
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

#[cfg(test)]
mod test {
    use super::super::parse_error::{back_n_newlines, forward_n_newlines};
    use super::{ParseError, Parseable};
    use kdl::KdlDocument;
    use pretty_assertions::assert_eq;
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
        assert_matches!(numbers.int_seq::<u8>(), Err(ParseError::InvalidSpan { .. }));
    }

    #[test]
    fn int_seq_rejects_string_props() {
        let doc: KdlDocument = "numbers 1 two three 4".parse().unwrap();
        let numbers = doc.nodes().first().unwrap();
        assert_matches!(numbers.int_seq::<u8>(), Err(ParseError::InvalidSpan { .. }));
    }

    #[test]
    fn int_seq_rejects_incompatible_numbers() {
        let doc: KdlDocument = "numbers 254 255 256 257".parse().unwrap();
        let numbers = doc.nodes().first().unwrap();
        assert_matches!(numbers.int_seq::<u8>(), Err(ParseError::InvalidSpan { .. }));
    }

    #[test]
    fn test_back_and_forward_newlines() {
        let haystack = "Line 1
Line 2
Line 3
Line 4
Line 5
Line 6
Line 7
Line 8
Line 9
Line 10";
        {
            let needle = "5";
            let span_start = haystack.find(needle).unwrap();
            let span_end = span_start + needle.len();

            let start = back_n_newlines(3, haystack, span_start);
            let end = forward_n_newlines(3, haystack, span_end);

            let expected_result = "Line 3
Line 4
Line 5
Line 6
Line 7";
            assert_eq!(expected_result, &haystack[start..end]);
        }

        {
            let needle = "2";
            let span_start = haystack.find(needle).unwrap();
            let span_end = span_start + needle.len();

            let start = back_n_newlines(3, haystack, span_start);
            let end = forward_n_newlines(3, haystack, span_end);

            let expected_result = "Line 1
Line 2
Line 3
Line 4";
            assert_eq!(expected_result, &haystack[start..end]);
        }

        {
            let needle = "9";
            let span_start = haystack.find(needle).unwrap();
            let span_end = span_start + needle.len();

            let start = back_n_newlines(3, haystack, span_start);
            let end = forward_n_newlines(3, haystack, span_end);

            let expected_result = "Line 7
Line 8
Line 9
Line 10";
            assert_eq!(expected_result, &haystack[start..end]);
        }
    }
}
