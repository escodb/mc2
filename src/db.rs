use std::cell::RefCell;
use std::collections::BTreeSet;

use crate::path::Path;
use crate::store::{Cache, Rev, Store};

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

pub type DbCache<'a, T> = Cache<'a, Path, Db<T>>;
pub type DbStore<T> = Store<Path, Db<T>>;

pub struct Checker<'a, T> {
    store: &'a RefCell<DbStore<T>>,
    seq: Rev,
    errors: Vec<String>,
}

impl<T> Checker<'_, T>
where
    T: Clone,
{
    pub fn new(store: &RefCell<DbStore<T>>) -> Checker<T> {
        Checker {
            store,
            seq: 0,
            errors: Vec::new(),
        }
    }

    pub fn check(&mut self) -> Result<(), Vec<String>> {
        let store = self.store.borrow();

        if self.seq == store.seq {
            return Ok(());
        }
        self.errors = Vec::new();

        for path in store.keys() {
            if path.is_doc() && store.get(path).is_some() {
                self.check_doc(&path);
            }
        }

        if self.errors.is_empty() {
            self.seq = store.seq;
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    fn check_doc(&mut self, doc: &Path) {
        for (dir, name) in doc.links() {
            if let Some(Db::Dir(entries)) = self.store.borrow().get(dir) {
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
    use crate::config::Config;

    fn make_store() -> DbStore<char> {
        let mut store = DbStore::new(Config::new());

        store.write("/".into(), None, Db::dir_from(&["path/"]));
        store.write("/path/".into(), None, Db::dir_from(&["to/"]));
        store.write("/path/to/".into(), None, Db::dir_from(&["x.json"]));
        store.write("/path/to/x.json".into(), None, Db::Doc('a'));

        store
    }

    #[test]
    fn checks_a_valid_store() {
        let store = make_store();

        let store_cell = RefCell::new(store);
        let mut checker = Checker::new(&store_cell);

        assert_eq!(checker.check(), Ok(()));
    }

    #[test]
    fn complains_if_a_doc_is_not_linked() {
        let mut store = make_store();
        store.write("/path/to/".into(), Some(1), Db::dir_from(&[]));

        let store_cell = RefCell::new(store);
        let mut checker = Checker::new(&store_cell);

        assert_eq!(
            checker.check(),
            Err(vec![String::from(
                "dir '/path/to/' does not include name 'x.json', required by doc '/path/to/x.json'"
            )])
        );
    }

    #[test]
    fn complains_if_a_parent_dir_is_deleted() {
        let mut store = make_store();
        store.remove("/path/to/".into(), Some(1));

        let store_cell = RefCell::new(store);
        let mut checker = Checker::new(&store_cell);

        assert_eq!(
            checker.check(),
            Err(vec![String::from(
                "dir '/path/to/', required by doc '/path/to/x.json', is missing"
            )])
        );
    }

    #[test]
    fn complains_if_parent_dir_is_missing() {
        let mut store = make_store();
        store.write("/".into(), Some(1), Db::dir_from(&["other/", "path/"]));
        store.write("/other/y.json".into(), None, Db::Doc('b'));

        let store_cell = RefCell::new(store);
        let mut checker = Checker::new(&store_cell);

        assert_eq!(
            checker.check(),
            Err(vec![String::from(
                "dir '/other/', required by doc '/other/y.json', is missing"
            )])
        );
    }

    #[test]
    fn complains_if_a_parent_dir_is_not_linked() {
        let mut store = make_store();
        store.write("/path/".into(), Some(1), Db::dir_from(&[]));

        let store_cell = RefCell::new(store);
        let mut checker = Checker::new(&store_cell);

        assert_eq!(
            checker.check(),
            Err(vec![String::from(
                "dir '/path/' does not include name 'to/', required by doc '/path/to/x.json'"
            )])
        );
    }

    #[test]
    fn complains_if_a_grandparent_dir_is_not_linked() {
        let mut store = make_store();
        store.write("/".into(), Some(1), Db::dir_from(&[]));

        let store_cell = RefCell::new(store);
        let mut checker = Checker::new(&store_cell);

        assert_eq!(
            checker.check(),
            Err(vec![String::from(
                "dir '/' does not include name 'path/', required by doc '/path/to/x.json'"
            )])
        );
    }

    #[test]
    fn does_not_complain_if_an_ancestor_of_a_deleted_doc_is_unlinked() {
        let mut store = make_store();
        store.write("/".into(), Some(1), Db::dir_from(&[]));
        store.remove("/path/to/x.json".into(), Some(1));

        let store_cell = RefCell::new(store);
        let mut checker = Checker::new(&store_cell);

        assert_eq!(checker.check(), Ok(()));
    }
}
