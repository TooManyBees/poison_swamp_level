use super::config::{Config, LogTarget};
use super::time::Time;
use anstyle::Style;
use env_logger::fmt::{Formatter, Target, WriteStyle};
use log::kv;
use std::time::SystemTime;
use std::{io, io::Write};

pub fn init_logger(config: &Config) {
    let target = match config.logging.target {
        LogTarget::Stdout => Target::Stdout,
        LogTarget::Stderr => Target::Stderr,
    };

    let mut builder = env_logger::builder();
    let logger = builder
        .target(target)
        .filter_level(config.logging.level)
        .format(move |formatter, record| {
            let now = SystemTime::now();
            let t = Time::from(now);
            let level = record.level();
            let target = record.target();
            let args = record.args();
            let level_style = formatter.default_level_style(level);
            write!(
                formatter,
                "{t} [{level_style}{level:<5}{level_style:#} {target}] {args}"
            )?;

            record
                .key_values()
                .visit(&mut Visitor { formatter })
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            write!(formatter, "\n")
        });

    let write_style = if config.logging.color {
        WriteStyle::Auto
    } else {
        WriteStyle::Never
    };

    logger.write_style(write_style).init();
}

struct Visitor<'a> {
    formatter: &'a mut Formatter,
}

static NEEDS_ESCAPE: &[char] = &[' ', '"', '\\', '\n'];
const KEY_STYLE: Style = Style::new().bold();

impl<'kvs> kv::VisitSource<'kvs> for Visitor<'_> {
    fn visit_pair(&mut self, key: kv::Key<'_>, value: kv::Value<'kvs>) -> Result<(), kv::Error> {
        write!(self.formatter, " {KEY_STYLE}{key}{KEY_STYLE:#}=")?;
        let value_str = value.to_string();
        if value_str.chars().any(|c| NEEDS_ESCAPE.contains(&c)) {
            let escaped_value = value_str.escape_debug();
            write!(self.formatter, "\"{escaped_value}\"")?
        } else {
            write!(self.formatter, "{value}")?
        }
        Ok(())
    }
}
