use aho_corasick::{AhoCorasick, BuildError, Input, Match};
use serde::de::{Deserialize, Deserializer, IgnoredAny, MapAccess, Visitor};
use std::{fmt, fs::File, path::Path};

pub struct RobotsJson {
    patterns: Vec<String>,
    matcher: AhoCorasick,
}

impl fmt::Debug for RobotsJson {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("RobotsJson")
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

impl RobotsJson {
    pub fn find<'h, I: Into<Input<'h>>>(&self, input: I) -> Option<Match> {
        self.matcher.find(input)
    }
}

pub fn load_robots_json<P: AsRef<Path>>(path: P) -> Result<RobotsJson, RobotsJsonError> {
    let f = File::open(path)?;
    let robots: UserAgents = serde_json::from_reader(f)?;
    let matcher = AhoCorasick::new(&robots.0)?;
    Ok(RobotsJson {
        patterns: robots.0,
        matcher,
    })
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

struct UserAgents(Vec<String>);

struct RobotsVisitor;

impl<'de> Visitor<'de> for RobotsVisitor {
    type Value = UserAgents;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an arbitrary map at the root level")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut vec = Vec::with_capacity(access.size_hint().unwrap_or(0));

        while let Some((key, _)) = access.next_entry::<String, IgnoredAny>()? {
            vec.push(key);
        }

        Ok(UserAgents(vec))
    }
}

impl<'de> Deserialize<'de> for UserAgents {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(RobotsVisitor)
    }
}

#[cfg(test)]
mod test {
    use super::UserAgents;

    #[test]
    fn serde_deserializes_object_keys() {
        let json = r#"{
            "key1": "literally anything",
            "key2": 0,
            "key3": { "literally": "anything" },
            "key4": ["but", "watch", "those", "trailing", "commas"]
        }"#;

        let user_agents: UserAgents = serde_json::from_str(json).unwrap();
        assert_eq!(user_agents.0, vec!["key1", "key2", "key3", "key4"]);
    }
}
