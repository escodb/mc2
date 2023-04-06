#![allow(dead_code)]

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::BTreeSet;

use crate::config::Config;
use crate::db::{Db, DbCache, DbStore};
use crate::path::Path;
use crate::planner::{Act, Op};

pub struct Actor<'a, T> {
    cache: DbCache<'a, T>,
    config: Config,
    crashed: bool,
    unlinks: BTreeSet<String>,
}

impl<T> Actor<'_, T>
where
    T: Clone,
{
    pub fn new(store: &RefCell<DbStore<T>>, config: Config) -> Actor<T> {
        Actor {
            cache: DbCache::new(store),
            config,
            crashed: false,
            unlinks: BTreeSet::new(),
        }
    }

    pub fn dispatch(&mut self, act: &Act<T>) {
        match &act.op {
            Op::Get => {
                self.get(&act.path);
            }
            Op::Put(update) => {
                self.put(&act.path, update);
            }
            Op::Rm => {
                self.rm(&act.path);
            }
            Op::List => {
                self.list(&act.path);
            }
            Op::Link(name) => {
                self.link(&act.path, name);
            }
            Op::Unlink(name) => {
                self.unlink(&act.path, name);
            }
        }
    }

    fn get(&mut self, path: &Path) -> Option<T> {
        if self.crashed {
            return None;
        }
        if let Some(Db::Doc(value)) = self.cache.read(path) {
            Some(value)
        } else {
            None
        }
    }

    fn put<F>(&mut self, path: &Path, update: F)
    where
        F: Fn(Option<T>) -> Option<T>,
    {
        if !self.crashed {
            if let Some(value) = update(self.get(path)) {
                self.write(path, Db::Doc(value));
            }
        }
    }

    fn rm(&mut self, path: &Path) {
        if self.crashed || self.get(path).is_none() {
            return;
        }

        if !self.cache.remove(path) {
            self.crashed = true;
            return;
        }

        self.unlinks = BTreeSet::new();

        for (dir, name) in path.links().rev() {
            self.unlinks.insert(dir.to_string());

            if self.list(dir) != Some(BTreeSet::from([name.to_string()])) {
                break;
            }
        }
    }

    fn list<'a, P>(&mut self, path: &'a P) -> Option<BTreeSet<String>>
    where
        Path: Borrow<P>,
        P: Ord + ?Sized,
        &'a P: Into<Path>,
    {
        if self.crashed {
            return None;
        }
        if let Some(Db::Dir(value)) = self.cache.read(path) {
            Some(value)
        } else {
            None
        }
    }

    fn link(&mut self, path: &Path, entry: &str) {
        if !self.crashed {
            let mut entries = self.list(path).unwrap_or_else(|| BTreeSet::new());
            if !self.config.skip_links || !entries.contains(entry) {
                entries.insert(entry.to_string());
                self.write(path, Db::Dir(entries));
            }
        }
    }

    fn unlink(&mut self, path: &Path, entry: &str) {
        if !self.crashed && self.unlinks.contains(path.full()) {
            let mut entries = self.list(path).unwrap_or_else(|| BTreeSet::new());
            entries.remove(entry);
            self.write(path, Db::Dir(entries));
        }
    }

    fn write(&mut self, key: &Path, value: Db<T>) {
        if !self.cache.write(key, value) {
            self.crashed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x_path() -> Path {
        Path::from("/path/x.json")
    }

    fn y_path() -> Path {
        Path::from("/path/to/y.json")
    }

    fn make_store() -> RefCell<DbStore<Vec<char>>> {
        let mut store = DbStore::new(Config::new());

        store.write("/".into(), None, Db::dir_from(&["path/"]));
        store.write("/path/".into(), None, Db::dir_from(&["to/", "x.json"]));
        store.write("/path/to/".into(), None, Db::dir_from(&["y.json"]));

        store.write(x_path(), None, Db::Doc(vec!['a', 'b']));
        store.write(y_path(), None, Db::Doc(vec!['c', 'd', 'e']));

        RefCell::new(store)
    }

    #[test]
    fn gets_an_existing_document() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        let doc = actor.get(&x_path());
        assert_eq!(doc, Some(vec!['a', 'b']));
    }

    #[test]
    fn returns_none_for_a_missing_document() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        let doc = actor.get(&"/y.json".into());
        assert_eq!(doc, None);
    }

    #[test]
    fn updates_a_document() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        actor.get(&x_path());
        actor.put(&x_path(), |doc| Some(doc?.iter().rev().cloned().collect()));

        let rec = store.borrow().read(&x_path());
        assert_eq!(rec, Some((2, Some(Db::Doc(vec!['b', 'a'])))));

        let doc = actor.get(&x_path());
        assert_eq!(doc, Some(vec!['b', 'a']));
    }

    #[test]
    fn updates_a_document_multiple_times() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        actor.get(&x_path());
        actor.put(&x_path(), |doc| Some(doc?.iter().rev().cloned().collect()));

        actor.put(&x_path(), |doc| {
            doc.map(|mut d| {
                d.push('z');
                d
            })
        });

        let rec = store.borrow().read(&x_path());
        assert_eq!(rec, Some((3, Some(Db::Doc(vec!['b', 'a', 'z'])))));

        let doc = actor.get(&x_path());
        assert_eq!(doc, Some(vec!['b', 'a', 'z']));
    }

    #[test]
    fn fails_to_write_a_conflicting_update() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        actor.get(&x_path());

        store
            .borrow_mut()
            .write(x_path(), Some(1), Db::Doc(vec!['z']));

        actor.put(&x_path(), |_| Some(vec!['p', 'q']));

        let rec = store.borrow().read(&x_path());
        assert_eq!(rec, Some((2, Some(Db::Doc(vec!['z'])))));
    }

    #[test]
    fn does_not_perform_more_actions_after_a_failed_write() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        actor.get(&x_path());

        store
            .borrow_mut()
            .write(x_path(), Some(1), Db::Doc(vec!['z']));

        actor.put(&x_path(), |_| Some(vec!['p', 'q']));

        assert_eq!(actor.get(&x_path()), None);
        actor.put(&x_path(), |_| Some(vec!['x', 'y']));

        let rec = store.borrow().read(&x_path());
        assert_eq!(rec, Some((2, Some(Db::Doc(vec!['z'])))));
    }

    #[test]
    fn creates_links() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        actor.link(&"/path/".into(), "a.txt");
        actor.link(&"/path/".into(), "z.txt");

        let rec = store.borrow().read("/path/");
        assert_eq!(
            rec,
            Some((3, Some(Db::dir_from(&["a.txt", "to/", "x.json", "z.txt"]))))
        );
    }

    #[test]
    fn creates_links_that_already_exist() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        actor.link(&"/path/".into(), "x.json");

        let rec = store.borrow().read("/path/");
        assert_eq!(rec, Some((2, Some(Db::dir_from(&["to/", "x.json"])))));
    }

    #[test]
    fn can_skip_creating_links_that_already_exist() {
        let store = make_store();
        let mut skipper = Actor::new(&store, Config::new().skip_links(true));

        skipper.link(&"/path/".into(), "x.json");

        let rec = store.borrow().read("/path/");
        assert_eq!(rec, Some((1, Some(Db::dir_from(&["to/", "x.json"])))));
    }

    #[test]
    fn does_not_skip_creating_links_that_do_not_exist() {
        let store = make_store();
        let mut skipper = Actor::new(&store, Config::new().skip_links(true));

        skipper.link(&"/path/".into(), "a.json");

        let rec = store.borrow().read("/path/");
        assert_eq!(
            rec,
            Some((2, Some(Db::dir_from(&["a.json", "to/", "x.json"]))))
        );
    }

    #[test]
    fn removes_a_document() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        actor.rm(&x_path());

        let rec = store.borrow().read(&x_path());
        assert_eq!(rec, Some((2, None)));
    }

    #[test]
    fn allows_empty_parent_directories_to_be_removed() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        actor.rm(&"/path/to/y.json".into());
        actor.unlink(&"/path/to/".into(), "y.json");
        actor.unlink(&"/path/".into(), "to/");
        actor.unlink(&"/".into(), "path/");

        assert_eq!(
            store.borrow().read("/"),
            Some((1, Some(Db::dir_from(&["path/"]))))
        );
        assert_eq!(
            store.borrow().read("/path/"),
            Some((2, Some(Db::dir_from(&["x.json"]))))
        );
        assert_eq!(
            store.borrow().read("/path/to/"),
            Some((2, Some(Db::dir_from(&[]))))
        );
        assert_eq!(store.borrow().read("/path/to/y.json"), Some((2, None)));
    }

    #[test]
    fn prevents_non_empty_parent_directories_being_removed() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        actor.rm(&"/path/x.json".into());
        actor.unlink(&"/path/".into(), "x.json");
        actor.unlink(&"/".into(), "path/");

        assert_eq!(
            store.borrow().read("/"),
            Some((1, Some(Db::dir_from(&["path/"]))))
        );
        assert_eq!(
            store.borrow().read("/path/"),
            Some((2, Some(Db::dir_from(&["to/"]))))
        );
        assert_eq!(store.borrow().read("/path/x.json"), Some((2, None)));
    }

    #[test]
    fn does_not_decide_to_remove_directories_by_default() {
        let store = make_store();
        let mut actor = Actor::new(&store, Config::new());

        actor.unlink(&"/path/to/".into(), "y.json");
        actor.unlink(&"/path/".into(), "to/");
        actor.unlink(&"/".into(), "path/");

        assert_eq!(
            store.borrow().read("/"),
            Some((1, Some(Db::dir_from(&["path/"]))))
        );
        assert_eq!(
            store.borrow().read("/path/"),
            Some((1, Some(Db::dir_from(&["to/", "x.json"]))))
        );
        assert_eq!(
            store.borrow().read("/path/to/"),
            Some((1, Some(Db::dir_from(&["y.json"]))))
        );
    }
}
