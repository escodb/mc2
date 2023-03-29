use std::collections::BTreeSet;

use crate::path::Path;
use crate::store::Store;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Db<T> {
    Doc(T),
    Dir(BTreeSet<String>),
}

impl<T> Db<T> {
    #[cfg(test)]
    pub fn dir_from(entries: &[&str]) -> Db<T> {
        let set = entries.iter().map(|s| s.to_string()).collect();
        Db::Dir(set)
    }
}

pub fn check_consistency<T>(store: &Store<Db<T>>) -> Result<(), Vec<String>>
where
    T: Clone,
{
    let mut checker = Checker {
        store,
        errors: Vec::new(),
    };

    checker.check();

    if checker.errors.is_empty() {
        Ok(())
    } else {
        Err(checker.errors)
    }
}

struct Checker<'a, T> {
    store: &'a Store<Db<T>>,
    errors: Vec<String>,
}

impl<T> Checker<'_, T>
where
    T: Clone,
{
    fn check(&mut self) {
        let paths = self.store.keys().map(|k| Path::from(k));

        for doc in paths.filter(|p| p.is_doc()) {
            if self.store.get(&doc).is_none() {
                continue;
            }
            self.check_doc(&doc);
        }
    }

    fn check_doc(&mut self, doc: &Path) {
        for (dir, name) in doc.links() {
            if let Some(Db::Dir(entries)) = self.store.get(dir) {
                if !entries.contains(name) {
                    self.errors.push(format!(
                        "dir '{}' does not include name '{}', required by doc '{}'",
                        dir, name, doc
                    ));
                }
            } else {
                self.errors.push(format!(
                    "dir '{}', required by doc '{}', is missing",
                    dir, doc
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> Store<Db<char>> {
        let mut store = Store::new();

        store.write("/", None, Db::dir_from(&["path/"]));
        store.write("/path/", None, Db::dir_from(&["to/"]));
        store.write("/path/to/", None, Db::dir_from(&["x.json"]));
        store.write("/path/to/x.json", None, Db::Doc('a'));

        store
    }

    #[test]
    fn checks_a_valid_store() {
        let store = make_store();
        assert_eq!(check_consistency(&store), Ok(()));
    }

    #[test]
    fn complains_if_a_doc_is_not_linked() {
        let mut store = make_store();
        store.write("/path/to/", Some(1), Db::dir_from(&[]));

        assert_eq!(
            check_consistency(&store),
            Err(vec![String::from(
                "dir '/path/to/' does not include name 'x.json', required by doc '/path/to/x.json'"
            )])
        );
    }

    #[test]
    fn complains_if_a_parent_dir_is_deleted() {
        let mut store = make_store();
        store.remove("/path/to/", Some(1));

        assert_eq!(
            check_consistency(&store),
            Err(vec![String::from(
                "dir '/path/to/', required by doc '/path/to/x.json', is missing"
            )])
        );
    }

    #[test]
    fn complains_if_parent_dir_is_missing() {
        let mut store = make_store();
        store.write("/", Some(1), Db::dir_from(&["other/", "path/"]));
        store.write("/other/y.json", None, Db::Doc('b'));

        assert_eq!(
            check_consistency(&store),
            Err(vec![String::from(
                "dir '/other/', required by doc '/other/y.json', is missing"
            )])
        );
    }

    #[test]
    fn complains_if_a_parent_dir_is_not_linked() {
        let mut store = make_store();
        store.write("/path/", Some(1), Db::dir_from(&[]));

        assert_eq!(
            check_consistency(&store),
            Err(vec![String::from(
                "dir '/path/' does not include name 'to/', required by doc '/path/to/x.json'"
            )])
        );
    }

    #[test]
    fn complains_if_a_grandparent_dir_is_not_linked() {
        let mut store = make_store();
        store.write("/", Some(1), Db::dir_from(&[]));

        assert_eq!(
            check_consistency(&store),
            Err(vec![String::from(
                "dir '/' does not include name 'path/', required by doc '/path/to/x.json'"
            )])
        );
    }

    #[test]
    fn does_not_complain_if_an_ancestor_of_a_deleted_doc_is_unlinked() {
        let mut store = make_store();
        store.write("/", Some(1), Db::dir_from(&[]));
        store.remove("/path/to/x.json", Some(1));

        assert_eq!(check_consistency(&store), Ok(()));
    }
}
