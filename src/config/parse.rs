use super::config::{Classifier, Config, Garbage, Logging, Server, ServerMode};
use http::status::StatusCode;
use kdl::{KdlDocument, KdlEntry, KdlError, KdlNode};
use log::LevelFilter;
use std::fmt::Write;
use std::{error::Error, fmt, fs, ops::Deref, path::Path, str::FromStr};

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, ParseError> {
    let source = fs::read_to_string(path)?;
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
                config.server = node.parse_server()?;
            }
            "classifier" => {
                config.classifier = node.parse_classifier()?;
            }
            "garbage" => {
                config.garbage = node.parse_garbage()?;
            }
            "logging" => {
                config.logging = node.parse_logging()?;
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

    fn parse_logging(&self) -> Result<Logging, ParseError>;

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
    fn parse_server(&self) -> Result<Server, ParseError> {
        let mut server = Server::default();

        let mode_entry = self.one_string_entry()?;
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

        for child in self.iter_children() {
            match child.name().value() {
                "listen" => {
                    let entry = child.one_string_entry()?;
                    server.listen = entry.as_ref().parse().map_err(|_| {
                        ParseError::from_entry(&entry, "invalid socket address".into())
                    })?;
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

    fn parse_classifier(&self) -> Result<Classifier, ParseError> {
        let mut classifier = Classifier::default();

        for child in self.iter_children() {
            match child.name().value() {
                "trusted-decision-header" => {
                    classifier.trusted_decision_header = Some(child.one_string_arg()?);
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

    fn parse_garbage(&self) -> Result<Garbage, ParseError> {
        let mut garbage = Garbage::default();

        for child in self.iter_children() {
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

    fn parse_logging(&self) -> Result<Logging, ParseError> {
        let mut logging = Logging::default();

        for child in self.iter_children() {
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
                "request-handler" => {
                    logging.request_handler = child.one_booleanish_entry()?;
                }
                _ => {}
            }
        }

        Ok(logging)
    }

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

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Kdl(KdlError),
    InvalidBlock {
        source: Option<String>,
        span: (usize, usize),
        message: String,
    },
    InvalidSpan {
        source: Option<String>,
        span: (usize, usize),
        message: String,
    },
}

impl ParseError {
    fn from_node(node: &KdlNode, message: String) -> ParseError {
        let span = node.span();
        ParseError::InvalidBlock {
            source: None,
            span: (span.offset(), span.offset() + span.len()),
            message,
        }
    }

    fn from_node_name(node: &KdlNode, message: String) -> ParseError {
        let span = node.span();
        let name = node.name().value();
        ParseError::InvalidSpan {
            source: None,
            span: (span.offset(), span.offset() + name.len()),
            message,
        }
    }

    fn from_entry(entry: &KdlEntry, message: String) -> ParseError {
        let span = entry.span();
        ParseError::InvalidSpan {
            source: None,
            span: (span.offset(), span.offset() + span.len()),
            message,
        }
    }

    fn set_source_code(&mut self, new_source: String) {
        match self {
            ParseError::InvalidSpan { source, .. } => {
                source.replace(new_source);
            }
            ParseError::InvalidBlock { source, .. } => {
                source.replace(new_source);
            }
            _ => {}
        }
    }

    pub fn explain(self) -> Explain {
        match self {
            ParseError::Io(e) => Explain {
                location: None,
                message: e.to_string(),
            },
            ParseError::Kdl(e) => Explain {
                location: None,
                message: e.to_string(),
            },
            ParseError::InvalidBlock {
                source,
                span,
                message,
                ..
            } => Explain {
                location: source.zip(Some(span)),
                message: message.clone(),
            },
            ParseError::InvalidSpan {
                source,
                span,
                message,
            } => Explain {
                location: source.zip(Some(span)),
                message: message.clone(),
            },
        }
    }
}

pub struct Explain {
    location: Option<(String, (usize, usize))>,
    message: String,
}

#[derive(Debug)]
struct Location {
    start: usize,
    end: usize,
    line: usize,
    col: usize,
}

impl fmt::Display for Explain {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some((ref source, (start, end))) = self.location {
            let line = source[..start].lines().count().max(1);
            let col = start - back_n_newlines(1, source, start);
            let highlighted_span = &source[start..end];
            let snippet = expand_source(&source, (start, end));
            write!(
                f,
                "{} at {} on line {}:\n",
                self.message, highlighted_span, line
            )?;
            let location = Location {
                start,
                end,
                line,
                col,
            };
            annotate_span(f, snippet, line.saturating_sub(2).max(1), location)
        } else {
            f.write_str(&self.message)
        }
    }
}

fn expand_source(source: &str, (start, end): (usize, usize)) -> &str {
    let source_start = back_n_newlines(3, source, start);
    let source_end = forward_n_newlines(3, source, end);
    &source[source_start..source_end + 1]
}

fn back_n_newlines(count: usize, source: &str, from: usize) -> usize {
    let mut idx = from;
    for _ in 0..count {
        match source[..idx].rfind('\n') {
            Some(i) => idx = i,
            None => return 0,
        }
    }
    idx + 1
}

fn forward_n_newlines(count: usize, source: &str, from: usize) -> usize {
    let mut idx = from;
    for _ in 0..count {
        match source[idx..].find('\n') {
            Some(i) => idx = idx + i + 1,
            None => return source.len(),
        }
    }
    idx - 1
}

fn annotate_span(
    f: &mut fmt::Formatter,
    source: &str,
    starting_line: usize,
    location: Location,
) -> fmt::Result {
    let max_line = location.line + 2; // FIXME
    let num_cols = max_line.checked_ilog10().unwrap_or(1).max(1) as usize;
    f.write_char('\n')?;
    for (n, line) in source.lines().enumerate() {
        let line_no = n + starting_line;
        write!(f, "{line_no:width$}  {line}\n", width = num_cols)?;
        if line_no == location.line {
            f.write_str(&" ".repeat(num_cols + 2 + location.col))?;
            f.write_str(&"^".repeat(location.end - location.start))?;
            f.write_char('\n')?;
        }
    }
    Ok(())
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

impl Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::Io(e) => e.fmt(f),
            ParseError::Kdl(e) => e.fmt(f),
            ParseError::InvalidBlock {
                message,
                source,
                span,
                ..
            } => {
                if let Some(source) = source {
                    write!(f, "{} at {}", message, &source[span.0..span.1])
                } else {
                    f.write_str(message)
                }
            }
            ParseError::InvalidSpan {
                message,
                source,
                span,
            } => {
                if let Some(source) = source {
                    write!(f, "{} at {}", message, &source[span.0..span.1])
                } else {
                    f.write_str(message)
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::{ParseError, Parseable, back_n_newlines, forward_n_newlines};
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
