use std::ops::RangeInclusive;

#[derive(Debug, Default)]
pub struct Config {
    pub garbage: Garbage,
}

#[derive(Debug, Default)]
pub struct Garbage {
    pub source_files: Vec<String>,
    pub words_file: Option<String>,
    pub paragraphs: Paragraphs,
    pub links: Links,
    pub template_file: Option<String>,
    pub poisons: Vec<String>,
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

#[derive(Debug)]
pub struct Links {
    min_words: usize,
    max_words: usize,
    min_count: usize,
    max_count: usize,
    pub separator: char,
}

impl Links {
    pub fn num_words(&self) -> RangeInclusive<usize> {
        self.min_words..=self.max_words
    }

    pub fn count(&self) -> RangeInclusive<usize> {
        self.min_count..=self.max_count
    }
}

impl Default for Links {
    fn default() -> Self {
        Links {
            min_words: 2,
            max_words: 4,
            min_count: 2,
            max_count: 5,
            separator: '-',
        }
    }
}
