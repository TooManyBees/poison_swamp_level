mod classifier;
mod config;
mod garbage;
pub mod metrics;
pub mod service;

pub use classifier::{Classification, Classifier, Decision};
pub use config::{Config, config_path, init_logger};
pub use garbage::{Corpus, Garbage};

fn http_path_extname(p: &str) -> Option<&str> {
    last_dot(p).map(|i| &p[i + 1..])
}

fn http_path_without_extname(p: &str) -> &str {
    last_dot(p).map(|i| &p[..i]).unwrap_or(p)
}

fn last_dot(p: &str) -> Option<usize> {
    let last_segment = match p.rfind('/') {
        None => 0,
        Some(last_slash) => last_slash + 1,
    };
    match p[last_segment..].rfind('.') {
        Some(0) => None,
        Some(i) => Some(last_segment + i),
        None => None,
    }
}

#[cfg(test)]
mod test {
    use super::{http_path_extname, http_path_without_extname};

    #[test]
    fn test_http_path_extname() {
        assert_eq!(http_path_extname(""), None);
        assert_eq!(http_path_extname("/"), None);
        assert_eq!(http_path_extname("hi"), None);
        assert_eq!(http_path_extname("hi.ext"), Some("ext"));
        assert_eq!(http_path_extname("/hi/there"), None);
        assert_eq!(http_path_extname("/hi/there/"), None);
        assert_eq!(http_path_extname("path/file.ext"), Some("ext"));
        assert_eq!(http_path_extname("/path/file.ext"), Some("ext"));
        assert_eq!(http_path_extname("/path/file.ext1.ext2"), Some("ext2"));
        assert_eq!(http_path_extname("/path/file.ext1/file.ext2"), Some("ext2"));
        assert_eq!(http_path_extname(".well-known"), None);
        assert_eq!(http_path_extname("/.well-known"), None);
    }

    #[test]
    fn test_http_path_without_extname() {
        assert_eq!(http_path_without_extname(""), "");
        assert_eq!(http_path_without_extname("/"), "/");
        assert_eq!(http_path_without_extname("hi"), "hi");
        assert_eq!(http_path_without_extname("hi.ext"), "hi");
        assert_eq!(http_path_without_extname("/hi/there"), "/hi/there");
        assert_eq!(http_path_without_extname("/hi/there/"), "/hi/there/");
        assert_eq!(http_path_without_extname("path/file.ext"), "path/file");
        assert_eq!(http_path_without_extname("/path/file.ext"), "/path/file");
        assert_eq!(
            http_path_without_extname("/path/file.ext1.ext2"),
            "/path/file.ext1"
        );
        assert_eq!(
            http_path_without_extname("/path/file.ext1/file.ext2"),
            "/path/file.ext1/file"
        );
        assert_eq!(http_path_without_extname(".well-known"), ".well-known");
        assert_eq!(http_path_without_extname("/.well-known"), "/.well-known");
    }
}
