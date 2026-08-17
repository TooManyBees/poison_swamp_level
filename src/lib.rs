mod classifier;
mod config;
mod garbage;

pub use classifier::{Classification, Classifier, TrustedDecision};
pub use config::Config;
pub use garbage::{Corpus, Garbage};
