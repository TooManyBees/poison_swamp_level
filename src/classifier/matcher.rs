use aho_corasick::{AhoCorasick, BuildError, Input, Match};
use std::fmt;

pub struct Matcher {
    patterns: Vec<String>,
    matcher: AhoCorasick,
}

impl fmt::Debug for Matcher {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Matcher")
            .field("patterns", &self.patterns)
            .field(
                "matcher",
                &format_args!(
                    "AhoCorasick {{ memory usage: {} }}",
                    self.matcher.memory_usage()
                ),
            )
            .finish()
    }
}

impl Matcher {
    pub fn new(patterns: Vec<String>) -> Result<Matcher, BuildError> {
        let matcher = AhoCorasick::new(&patterns)?;
        Ok(Matcher { patterns, matcher })
    }

    pub fn find<'h, I: Into<Input<'h>>>(&self, input: I) -> Option<Match> {
        self.matcher.find(input)
    }
}
