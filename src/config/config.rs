use std::ops::RangeInclusive;

#[derive(Debug, Default)]
pub struct Config {
    pub garbage: Garbage,
}

#[derive(Debug, Default)]
pub struct Garbage {
    pub source_files: Vec<String>,
    pub paragraphs: Paragraphs,
    pub template_file: Option<String>,
}

#[derive(Debug)]
pub struct Paragraphs {
    min_words: usize,
    max_words: usize,
    min_count: usize,
    max_count: usize,
}

impl Paragraphs {
    pub fn num_words(&self) -> RangeInclusive<usize> {
        self.min_words..=self.max_words
    }

    pub fn count(&self) -> RangeInclusive<usize> {
        self.min_count..=self.max_count
    }
}

impl Default for Paragraphs {
    fn default() -> Self {
        Paragraphs {
            min_words: 16,
            max_words: 32,
            min_count: 4,
            max_count: 6,
        }
    }
}
