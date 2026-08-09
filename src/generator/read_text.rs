use crate::generator::Substring;
use std::str::CharIndices;

pub struct Substrings<'a> {
    inner: CharIndices<'a>,
}

impl<'a> Substrings<'a> {
    pub fn new(s: &'a str) -> Self {
        Substrings {
            inner: s.char_indices(),
        }
    }

    pub fn windows(self) -> SubstringsWindows<'a> {
        SubstringsWindows::new(self)
    }
}

impl<'a> Iterator for Substrings<'a> {
    type Item = Substring;

    fn next(&mut self) -> Option<Self::Item> {
        let a = loop {
            let (idx, c) = self.inner.next()?;
            if !c.is_whitespace() {
                break idx;
            }
        };

        let b = loop {
            match self.inner.next() {
                Some((idx, c)) => {
                    if c.is_whitespace() {
                        break idx;
                    }
                }
                None => {
                    break self.inner.offset();
                }
            }
        };

        Some(Substring(a, b))
    }
}

#[derive(Copy, Clone, Debug)]
enum WindowState {
    At0,
    At1,
    At2,
}

impl WindowState {
    fn next(self) -> Self {
        match self {
            WindowState::At0 => WindowState::At1,
            WindowState::At1 => WindowState::At2,
            WindowState::At2 => WindowState::At0,
        }
    }

    fn as_index(self) -> usize {
        match self {
            WindowState::At0 => 0,
            WindowState::At1 => 1,
            WindowState::At2 => 2,
        }
    }
}

pub struct SubstringsWindows<'a> {
    inner: Substrings<'a>,
    window: [Substring; 3],
    state: WindowState,
}

impl<'a> SubstringsWindows<'a> {
    fn new(mut inner: Substrings<'a>) -> Self {
        let mut window = [Substring(0, 0); 3];
        window[0] = inner.next().unwrap_or_default();
        window[1] = inner.next().unwrap_or_default();

        SubstringsWindows {
            inner,
            window,
            state: WindowState::At2,
        }
    }
}

impl<'a> Iterator for SubstringsWindows<'a> {
    type Item = (Substring, Substring, Substring);

    fn next(&mut self) -> Option<Self::Item> {
        self.window[self.state.as_index()] = self.inner.next()?;
        let window = match self.state {
            WindowState::At0 => (self.window[1], self.window[2], self.window[0]),
            WindowState::At1 => (self.window[2], self.window[0], self.window[1]),
            WindowState::At2 => (self.window[0], self.window[1], self.window[2]),
        };
        self.state = self.state.next();
        Some(window)
    }
}

#[cfg(test)]
mod test {
    use super::{Substring, Substrings};

    #[test]
    fn substrings_parses_words() {
        let text = "this is some text";
        let substrings = Substrings::new(text).collect::<Vec<_>>();
        assert_eq!(
            substrings,
            vec!(
                Substring(0, 4),
                Substring(5, 7),
                Substring(8, 12),
                Substring(13, 17)
            )
        );
    }

    #[test]
    fn substrings_windows() {
        let text = "this is some text";
        let mut windows = Substrings::new(text).windows();
        assert_eq!(
            windows.next(),
            Some((Substring(0, 4), Substring(5, 7), Substring(8, 12)))
        );
        assert_eq!(
            windows.next(),
            Some((Substring(5, 7), Substring(8, 12), Substring(13, 17)))
        );
        assert_eq!(windows.next(), None);
    }

    #[test]
    fn substrings_windows_returns_none() {
        let text = "this is";
        let mut windows = Substrings::new(text).windows();
        assert_eq!(windows.next(), None);
    }
}
