#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::BTreeSet;

use crate::path::Path;
use crate::store::{Cache, Store};

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

pub struct Actor<'a, T> {
    cache: Cache<'a, Db<T>>,
    crashed: bool,
    unlinks: BTreeSet<String>,
}

impl<T> Actor<'_, T>
where
    T: Clone,
{
    pub fn new(store: &RefCell<Store<Db<T>>>) -> Actor<T> {
        Actor {
            cache: Cache::new(store),
            crashed: false,
            unlinks: BTreeSet::new(),
        }
    }

    pub fn get(&mut self, path: &str) -> Option<T> {
        if self.crashed {
            return None;
        }
        if let Some(Db::Doc(value)) = self.cache.read(path) {
            Some(value)
        } else {
            None
        }
    }

    pub fn put<F>(&mut self, path: &str, update: F)
    where
        F: Fn(Option<T>) -> Option<T>,
    {
        if !self.crashed {
            if let Some(value) = update(self.get(path)) {
                self.write(path, Db::Doc(value));
            }
        }
    }

    pub fn rm(&mut self, path: &str) {
        if self.crashed || self.get(path).is_none() {
            return;
        }

        if !self.cache.remove(path) {
            self.crashed = true;
            return;
        }

        self.unlinks = BTreeSet::new();

        for (dir, name) in Path::from(path).links().rev() {
            self.unlinks.insert(dir.into());

            if self.list(dir) != Some(BTreeSet::from([name.into()])) {
                break;
            }
        }
    }

    pub fn list(&mut self, path: &str) -> Option<BTreeSet<String>> {
        if self.crashed {
            return None;
        }
        if let Some(Db::Dir(value)) = self.cache.read(path) {
            Some(value)
        } else {
            None
        }
    }

    pub fn link(&mut self, path: &str, entry: &str) {
        if !self.crashed {
            let mut entries = self.list(path).unwrap_or_else(|| BTreeSet::new());
            entries.insert(entry.into());
            self.write(path, Db::Dir(entries));
        }
    }

    pub fn unlink(&mut self, path: &str, entry: &str) {
        if !self.crashed && self.unlinks.contains(path) {
            let mut entries = self.list(path).unwrap_or_else(|| BTreeSet::new());
            entries.remove(entry);
            self.write(path, Db::Dir(entries));
        }
    }

    fn write(&mut self, path: &str, value: Db<T>) {
        if !self.cache.write(path, value) {
            self.crashed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const X_PATH: &str = "/path/x.json";
    const Y_PATH: &str = "/path/to/y.json";

    fn make_store() -> RefCell<Store<Db<Vec<char>>>> {
        let mut store = Store::new();

        store.write("/", None, Db::dir_from(&["path/"]));
        store.write("/path/", None, Db::dir_from(&["to/", "x.json"]));
        store.write("/path/to/", None, Db::dir_from(&["y.json"]));

        store.write(X_PATH, None, Db::Doc(vec!['a', 'b']));
        store.write(Y_PATH, None, Db::Doc(vec!['c', 'd', 'e']));

        RefCell::new(store)
    }

    #[test]
    fn gets_an_existing_document() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        let doc = actor.get(X_PATH);
        assert_eq!(doc, Some(vec!['a', 'b']));
    }

    #[test]
    fn returns_none_for_a_missing_document() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        let doc = actor.get("/y.json");
        assert_eq!(doc, None);
    }

    #[test]
    fn updates_a_document() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        actor.get(X_PATH);
        actor.put(X_PATH, |doc| Some(doc?.iter().rev().cloned().collect()));

        let rec = store.borrow().read(X_PATH);
        assert_eq!(rec, Some((2, Db::Doc(vec!['b', 'a']))));

        let doc = actor.get(X_PATH);
        assert_eq!(doc, Some(vec!['b', 'a']));
    }

    #[test]
    fn updates_a_document_multiple_times() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        actor.get(X_PATH);
        actor.put(X_PATH, |doc| Some(doc?.iter().rev().cloned().collect()));

        actor.put(X_PATH, |doc| {
            doc.map(|mut d| {
                d.push('z');
                d
            })
        });

        let rec = store.borrow().read(X_PATH);
        assert_eq!(rec, Some((3, Db::Doc(vec!['b', 'a', 'z']))));

        let doc = actor.get(X_PATH);
        assert_eq!(doc, Some(vec!['b', 'a', 'z']));
    }

    #[test]
    fn fails_to_write_a_conflicting_update() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        actor.get(X_PATH);

        store
            .borrow_mut()
            .write(X_PATH, Some(1), Db::Doc(vec!['z']));

        actor.put(X_PATH, |_| Some(vec!['p', 'q']));

        let rec = store.borrow().read(X_PATH);
        assert_eq!(rec, Some((2, Db::Doc(vec!['z']))));
    }

    #[test]
    fn does_not_perform_more_actions_after_a_failed_write() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        actor.get(X_PATH);

        store
            .borrow_mut()
            .write(X_PATH, Some(1), Db::Doc(vec!['z']));

        actor.put(X_PATH, |_| Some(vec!['p', 'q']));

        assert_eq!(actor.get(X_PATH), None);
        actor.put(X_PATH, |_| Some(vec!['x', 'y']));

        let rec = store.borrow().read(X_PATH);
        assert_eq!(rec, Some((2, Db::Doc(vec!['z']))));
    }

    #[test]
    fn creates_links() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        actor.link("/path/", "a.txt");
        actor.link("/path/", "z.txt");

        let rec = store.borrow().read("/path/");
        assert_eq!(
            rec,
            Some((3, Db::dir_from(&["a.txt", "to/", "x.json", "z.txt"])))
        );
    }

    #[test]
    fn creates_links_that_already_exist() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        actor.link("/path/", "x.json");

        let rec = store.borrow().read("/path/");
        assert_eq!(rec, Some((2, Db::dir_from(&["to/", "x.json"]))));
    }

    #[test]
    fn can_skip_creating_links_that_already_exist() {
        // todo
    }

    #[test]
    fn does_not_skip_creating_links_that_do_not_exist() {
        // todo
    }

    #[test]
    fn removes_a_document() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        actor.rm(X_PATH);

        let rec = store.borrow().read(X_PATH);
        assert_eq!(rec, None);
    }

    #[test]
    fn allows_empty_parent_directories_to_be_removed() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        actor.rm("/path/to/y.json");
        actor.unlink("/path/to/", "y.json");
        actor.unlink("/path/", "to/");
        actor.unlink("/", "path/");

        assert_eq!(
            store.borrow().read("/"),
            Some((1, Db::dir_from(&["path/"])))
        );
        assert_eq!(
            store.borrow().read("/path/"),
            Some((2, Db::dir_from(&["x.json"])))
        );
        assert_eq!(
            store.borrow().read("/path/to/"),
            Some((2, Db::dir_from(&[])))
        );
        assert_eq!(store.borrow().read("/path/to/y.json"), None);
    }

    #[test]
    fn prevents_non_empty_parent_directories_being_removed() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        actor.rm("/path/x.json");
        actor.unlink("/path/", "x.json");
        actor.unlink("/", "path/");

        assert_eq!(
            store.borrow().read("/"),
            Some((1, Db::dir_from(&["path/"])))
        );
        assert_eq!(
            store.borrow().read("/path/"),
            Some((2, Db::dir_from(&["to/"])))
        );
        assert_eq!(store.borrow().read("/path/x.json"), None);
    }

    #[test]
    fn does_not_decide_to_remove_directories_by_default() {
        let store = make_store();
        let mut actor = Actor::new(&store);

        actor.unlink("/path/to/", "y.json");
        actor.unlink("/path/", "to/");
        actor.unlink("/", "path/");

        assert_eq!(
            store.borrow().read("/"),
            Some((1, Db::dir_from(&["path/"])))
        );
        assert_eq!(
            store.borrow().read("/path/"),
            Some((1, Db::dir_from(&["to/", "x.json"])))
        );
        assert_eq!(
            store.borrow().read("/path/to/"),
            Some((1, Db::dir_from(&["y.json"])))
        );
    }
}
