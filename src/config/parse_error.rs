use kdl::{KdlEntry, KdlError, KdlNode};
use std::{error::Error, fmt, fmt::Write, path::PathBuf};

#[derive(Debug)]
pub enum ParseError {
    Io(PathBuf, std::io::Error),
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
    // pub(super) fn from_node(node: &KdlNode, message: String) -> ParseError {
    //     let span = node.span();
    //     ParseError::InvalidBlock {
    //         source: None,
    //         span: (span.offset(), span.offset() + span.len()),
    //         message,
    //     }
    // }

    pub(super) fn from_node_name(node: &KdlNode, message: String) -> ParseError {
        let span = node.span();
        let name = node.name().value();
        ParseError::InvalidSpan {
            source: None,
            span: (span.offset(), span.offset() + name.len()),
            message,
        }
    }

    pub(super) fn from_entry(entry: &KdlEntry, message: String) -> ParseError {
        let span = entry.span();
        ParseError::InvalidSpan {
            source: None,
            span: (span.offset(), span.offset() + span.len()),
            message,
        }
    }

    pub(super) fn set_source_code(&mut self, new_source: String) {
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
            ParseError::Io(_, _) => Explain {
                location: None,
                message: format!("{self}"),
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

pub(super) fn back_n_newlines(count: usize, source: &str, from: usize) -> usize {
    let mut idx = from;
    for _ in 0..count {
        match source[..idx].rfind('\n') {
            Some(i) => idx = i,
            None => return 0,
        }
    }
    idx + 1
}

pub(super) fn forward_n_newlines(count: usize, source: &str, from: usize) -> usize {
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
    let num_cols = max_line.checked_ilog10().unwrap_or(0) as usize + 1;
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

impl From<KdlError> for ParseError {
    fn from(e: KdlError) -> ParseError {
        ParseError::Kdl(e)
    }
}

impl Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::Io(path, e) => write!(f, "Could not open file {}: {}", path.display(), e),
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
