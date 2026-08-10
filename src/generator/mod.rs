mod generator;
mod read_text;

pub use generator::Corpus;
use generator::{JOIN_AFTER, JOIN_BEFORE, State, Substring, is_ending};
use read_text::{ParseError, read, read_from_files, read_from_strings};
