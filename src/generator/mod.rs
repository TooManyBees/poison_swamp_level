mod generator;
mod read_text;

pub use generator::Generator;
use generator::{State, Substring};
use read_text::{ParseError, read, read_from_files, read_from_strings};
