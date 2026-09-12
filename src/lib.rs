mod classifier;
mod config;
mod garbage;
pub mod metrics;
pub mod service;

pub use classifier::{Classification, Classifier, Decision};
pub use config::{Config, init_logger};
pub use garbage::{Corpus, Garbage};
