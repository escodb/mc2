#![allow(dead_code)]

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;

const SEP: char = '/';

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Path {
    original: String,
    parts: Vec<(String, String)>,
}

impl fmt::Debug for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Path({})", &self.original)
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.original)
    }
}

impl Borrow<str> for Path {
    fn borrow(&self) -> &str {
        &self.original
    }
}

impl Deref for Path {
    type Target = str;

    fn deref(&self) -> &str {
        &self.original
    }
}

impl From<&str> for Path {
    fn from(value: &str) -> Path {
        Path::new(value)
    }
}

fn parse(path: &str) -> Vec<(String, String)> {
    let mut parts: Vec<_> = path.split(SEP).map(|s| s.to_string()).collect();
    let len = parts.len();

    for part in &mut parts[0..len - 1] {
        part.push(SEP);
    }
    if parts.last().map(|s| s.as_ref()) == Some("") {
        parts.pop();
    }

    let links = parts.iter().enumerate().skip(1);

    links
        .map(|(i, part)| (parts[0..i].join(""), part.clone()))
        .collect()
}

impl Path {
    pub fn new(name: &str) -> Path {
        Path {
            original: name.into(),
            parts: parse(name),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.original.starts_with(SEP)
    }

    pub fn is_dir(&self) -> bool {
        self.original.ends_with(SEP)
    }

    pub fn is_doc(&self) -> bool {
        !self.is_dir()
    }

    pub fn full(&self) -> &str {
        &self.original
    }

    pub fn dirs(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.parts.iter().map(|(dir, _)| dir.as_ref())
    }

    pub fn links(&self) -> impl DoubleEndedIterator<Item = (&str, &str)> {
        self.parts
            .iter()
            .map(|(dir, name)| (dir.as_ref(), name.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derefs_to_str() {
        let path = Path::from("/foo");
        assert_eq!(path.deref(), "/foo");
    }

    #[test]
    fn is_valid_if_it_begins_with_a_slash() {
        let path = Path::from("/foo");
        assert!(path.is_valid());
    }

    #[test]
    fn is_not_valid_if_it_does_not_begin_with_a_slash() {
        let path = Path::from("foo");
        assert!(!path.is_valid());
    }

    #[test]
    fn is_a_dir_if_it_ends_with_a_slash() {
        let path = Path::from("/foo/");
        assert!(path.is_dir());
    }

    #[test]
    fn is_not_a_dir_if_it_does_not_end_with_a_slash() {
        let path = Path::from("/foo");
        assert!(!path.is_dir());
    }

    #[test]
    fn is_a_doc_if_it_does_not_end_with_a_slash() {
        let path = Path::from("/foo");
        assert!(path.is_doc());
    }

    #[test]
    fn is_not_a_doc_if_it_ends_with_a_slash() {
        let path = Path::from("/foo/");
        assert!(!path.is_doc());
    }

    #[test]
    fn returns_the_full_path_for_a_document() {
        let path = Path::from("/path/to/x.json");
        assert_eq!(path.full(), "/path/to/x.json");
    }

    #[test]
    fn returns_the_full_path_for_a_directory() {
        let path = Path::from("/path/to/");
        assert_eq!(path.full(), "/path/to/");
    }

    #[test]
    fn returns_the_parent_directories_for_a_document() {
        let path = Path::from("/path/to/x.json");
        let dirs: Vec<_> = path.dirs().collect();

        assert_eq!(dirs, ["/", "/path/", "/path/to/"]);
    }

    #[test]
    fn returns_the_parent_directories_for_a_directory() {
        let path = Path::from("/path/to/");
        let dirs: Vec<_> = path.dirs().collect();

        assert_eq!(dirs, ["/", "/path/"]);
    }

    #[test]
    fn returns_the_required_links_for_a_document() {
        let path = Path::from("/path/to/x.json");
        let links: Vec<_> = path.links().collect();

        assert_eq!(
            links,
            [("/", "path/"), ("/path/", "to/"), ("/path/to/", "x.json")]
        );
    }

    #[test]
    fn returns_the_required_links_for_a_directory() {
        let path = Path::from("/path/to/");
        let links: Vec<_> = path.links().collect();

        assert_eq!(links, [("/", "path/"), ("/path/", "to/")]);
    }
}
