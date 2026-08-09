mod generator;
mod read_text;

pub use generator::Corpus;
use generator::{State, Substring};
use read_text::{ParseError, read, read_from_files, read_from_strings};
