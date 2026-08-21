mod classifier;
mod config;
mod garbage;

pub use classifier::{Classification, Classifier, Decision};
pub use config::{Config, ServerMode};
pub use garbage::{Corpus, Garbage};
