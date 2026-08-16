use aho_corasick::{AhoCorasick, BuildError, Input, Match};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct RobotsJsonEntry {
    operator: String,
    respect: String,
    function: String,
    frequency: String,
    description: String,
}

#[derive(Debug)]
pub struct RobotsJson {
    matcher: AhoCorasick,
}

impl RobotsJson {
    pub fn find<'h, I: Into<Input<'h>>>(&self, input: I) -> Option<Match> {
        self.matcher.find(input)
    }
}

pub fn load_robots_json<P: AsRef<Path>>(path: P) -> Result<RobotsJson, RobotsJsonError> {
    let f = File::open(path)?;
    let json: HashMap<String, RobotsJsonEntry> = serde_json::from_reader(f)?;
    let robots: Vec<_> = json.keys().cloned().collect();
    let matcher = AhoCorasick::new(&robots)?;
    Ok(RobotsJson { matcher })
}

#[derive(Debug)]
pub enum RobotsJsonError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Matcher(BuildError),
}

impl From<std::io::Error> for RobotsJsonError {
    fn from(e: std::io::Error) -> RobotsJsonError {
        RobotsJsonError::Io(e)
    }
}

impl From<serde_json::Error> for RobotsJsonError {
    fn from(e: serde_json::Error) -> RobotsJsonError {
        RobotsJsonError::Json(e)
    }
}

impl From<BuildError> for RobotsJsonError {
    fn from(e: BuildError) -> RobotsJsonError {
        RobotsJsonError::Matcher(e)
    }
}
