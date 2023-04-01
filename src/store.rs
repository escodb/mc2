use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::BTreeMap;

type Rev = usize;

#[derive(Clone)]
pub struct Store<K, V> {
    data: BTreeMap<K, (Rev, Option<V>)>,
    pub seq: Rev,
}

impl<K, V> Store<K, V>
where
    K: Ord,
    V: Clone,
{
    pub fn new() -> Store<K, V> {
        Store {
            data: BTreeMap::new(),
            seq: 0,
        }
    }

    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if let Some((_, Some(value))) = self.data.get(key) {
            Some(value)
        } else {
            None
        }
    }

    pub fn read<Q>(&self, key: &Q) -> Option<(Rev, V)>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if let Some((rev, Some(value))) = self.data.get(key) {
            Some((*rev, value.clone()))
        } else {
            None
        }
    }

    pub fn write<'a, Q>(&mut self, key: &'a Q, rev: Option<Rev>, value: V) -> Option<Rev>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
        &'a Q: Into<K>,
    {
        self.set_key(key, rev, Some(value))
    }

    pub fn remove<'a, Q>(&mut self, key: &'a Q, rev: Option<Rev>) -> Option<Rev>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
        &'a Q: Into<K>,
    {
        self.set_key(key, rev, None)
    }

    fn set_key<'a, Q>(&mut self, key: &'a Q, rev: Option<Rev>, value: Option<V>) -> Option<Rev>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
        &'a Q: Into<K>,
    {
        let client_rev = rev.unwrap_or(0);
        let entry = self.data.entry(key.into()).or_insert((0, None));

        if entry.0 != client_rev {
            return None;
        }

        *entry = (entry.0 + 1, value);
        self.seq += 1;

        Some(entry.0)
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.data.keys()
    }
}

pub struct Cache<'a, K, V> {
    store: &'a RefCell<Store<K, V>>,
    data: BTreeMap<K, Option<(Rev, V)>>,
}

impl<K, V> Cache<'_, K, V>
where
    K: Ord,
    V: Clone,
{
    pub fn new(store: &RefCell<Store<K, V>>) -> Cache<K, V> {
        Cache {
            store,
            data: BTreeMap::new(),
        }
    }

    pub fn read<'a, Q>(&mut self, key: &'a Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
        &'a Q: Into<K>,
    {
        if !self.data.contains_key(key) {
            let record = self.store.borrow().read(key);
            self.data.insert(key.into(), record);
        }

        if let Some(Some((_, value))) = self.data.get(key) {
            Some(value.clone())
        } else {
            None
        }
    }

    pub fn write<'a, Q>(&mut self, key: &'a Q, value: V) -> bool
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
        &'a Q: Into<K>,
    {
        let old_rev = self.get_rev(key);
        let mut store = self.store.borrow_mut();

        if let Some(new_rev) = store.write(key, old_rev, value.clone()) {
            self.data.insert(key.into(), Some((new_rev, value)));
            true
        } else {
            self.data.remove(key);
            false
        }
    }

    pub fn remove<'a, Q>(&mut self, key: &'a Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
        &'a Q: Into<K>,
    {
        let old_rev = self.get_rev(key);
        let mut store = self.store.borrow_mut();

        if let Some(_) = store.remove(key, old_rev) {
            self.data.insert(key.into(), None);
            true
        } else {
            self.data.remove(key);
            false
        }
    }

    fn get_rev<Q>(&self, key: &Q) -> Option<Rev>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if let Some(Some((rev, _))) = self.data.get(key) {
            Some(*rev)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_for_unknown_key() {
        let store: Store<String, ()> = Store::new();
        assert_eq!(store.seq, 0);
        assert_eq!(store.read("x"), None);
    }

    #[test]
    fn stores_a_new_value() {
        let mut store: Store<String, _> = Store::new();
        assert_eq!(store.write("x", None, 'a'), Some(1));
        assert_eq!(store.seq, 1);
        assert_eq!(store.read("x"), Some((1, 'a')));
    }

    #[test]
    fn does_not_update_a_value_without_a_rev() {
        let mut store: Store<String, _> = Store::new();
        store.write("x", None, 'a');

        assert_eq!(store.write("x", None, 'b'), None);
        assert_eq!(store.seq, 1);
        assert_eq!(store.read("x"), Some((1, 'a')));
    }

    #[test]
    fn does_not_update_a_value_with_a_bad_rev() {
        let mut store: Store<String, _> = Store::new();
        let rev = store.write("x", None, 'a').unwrap();

        assert_eq!(store.write("x", Some(rev + 1), 'b'), None);
        assert_eq!(store.seq, 1);
        assert_eq!(store.read("x"), Some((1, 'a')));
    }

    #[test]
    fn updates_a_value_with_a_matching_rev() {
        let mut store: Store<String, _> = Store::new();
        let rev = store.write("x", None, 'a').unwrap();

        assert_eq!(store.write("x", Some(rev), 'b'), Some(2));
        assert_eq!(store.seq, 2);
        assert_eq!(store.read("x"), Some((2, 'b')));
    }

    #[test]
    fn removes_a_value() {
        let mut store: Store<String, _> = Store::new();
        let rev = store.write("x", None, 'a').unwrap();

        assert_eq!(store.remove("x", Some(rev)), Some(2));
        assert_eq!(store.read("x"), None);
    }

    #[test]
    fn updates_a_different_key() {
        let mut store: Store<String, _> = Store::new();
        store.write("x", None, 'a').unwrap();

        assert_eq!(store.write("y", None, 'z'), Some(1));
        assert_eq!(store.seq, 2);
        assert_eq!(store.read("x"), Some((1, 'a')));
        assert_eq!(store.read("y"), Some((1, 'z')));
    }

    #[test]
    fn returns_a_copy_of_the_stored_value() {
        let mut store: Store<String, _> = Store::new();
        store.write("x", None, vec![4, 5, 6]);

        let (_, mut a) = store.read("x").unwrap();
        a.push(7);

        assert_eq!(store.read("x"), Some((1, vec![4, 5, 6])));
    }

    #[test]
    fn returns_all_the_keys_in_the_store() {
        let mut store: Store<String, _> = Store::new();

        store.write("/", None, 'a');
        store.write("/path/", None, 'b');
        store.write("/z/doc.json", None, 'c');

        let keys: Vec<_> = store.keys().collect();
        assert_eq!(keys, ["/", "/path/", "/z/doc.json"]);
    }

    #[test]
    fn returns_none_for_an_unknown_key() {
        let store: RefCell<Store<String, ()>> = RefCell::new(Store::new());
        let mut cache = Cache::new(&store);

        assert_eq!(cache.read("x"), None);
    }

    #[test]
    fn reads_a_value_from_the_store() {
        let store = RefCell::new(Store::new());
        let mut cache: Cache<String, _> = Cache::new(&store);

        assert_eq!(store.borrow_mut().write("x", None, 'a'), Some(1));
        assert_eq!(cache.read("x"), Some('a'));
    }

    #[test]
    fn returns_a_copy_of_the_cached_value() {
        let store = RefCell::new(Store::new());
        let mut cache: Cache<String, _> = Cache::new(&store);

        cache.write("x", vec![4, 5, 6]);

        let mut a = cache.read("x").unwrap();
        a.push(7);

        assert_eq!(cache.read("x"), Some(vec![4, 5, 6]));
    }

    #[test]
    fn caches_a_read_that_returns_none() {
        let store = RefCell::new(Store::new());
        let mut cache: Cache<String, _> = Cache::new(&store);

        assert_eq!(cache.read("x"), None);
        assert_eq!(store.borrow_mut().write("x", None, 'a'), Some(1));
        assert_eq!(cache.read("x"), None);
    }

    #[test]
    fn writes_a_value_to_the_store() {
        let store = RefCell::new(Store::new());
        let mut cache: Cache<String, _> = Cache::new(&store);

        assert_eq!(cache.write("x", 'a'), true);

        assert_eq!(store.borrow().read("x"), Some((1, 'a')));
        assert_eq!(cache.read("x"), Some('a'));
    }

    #[test]
    fn updates_a_value_in_the_store() {
        let store = RefCell::new(Store::new());
        let mut cache: Cache<String, _> = Cache::new(&store);

        assert_eq!(cache.write("x", 'a'), true);
        assert_eq!(cache.write("x", 'b'), true);
        assert_eq!(cache.write("x", 'c'), true);

        assert_eq!(store.borrow().read("x"), Some((3, 'c')));
        assert_eq!(cache.read("x"), Some('c'));
    }

    #[test]
    fn removes_a_value_from_the_store() {
        let store = RefCell::new(Store::new());
        let mut cache: Cache<String, _> = Cache::new(&store);

        assert_eq!(cache.write("x", 'a'), true);
        assert_eq!(cache.remove("x"), true);

        assert_eq!(store.borrow().read("x"), None);
        assert_eq!(cache.read("x"), None);
    }

    #[test]
    fn fails_to_update_a_doc_it_did_not_read_first() {
        let store = RefCell::new(Store::new());
        let mut cache: Cache<String, _> = Cache::new(&store);

        assert_eq!(store.borrow_mut().write("x", None, 'a'), Some(1));
        assert_eq!(cache.write("x", 'b'), false);

        assert_eq!(store.borrow().read("x"), Some((1, 'a')));
        assert_eq!(cache.read("x"), Some('a'));
    }

    #[test]
    fn fails_to_update_with_a_stale_read() {
        let store = RefCell::new(Store::new());
        let mut cache: Cache<String, _> = Cache::new(&store);

        assert_eq!(cache.write("x", 'a'), true);

        assert_eq!(store.borrow_mut().write("x", Some(1), 'c'), Some(2));
        assert_eq!(cache.write("x", 'b'), false);

        assert_eq!(store.borrow().read("x"), Some((2, 'c')));
    }

    #[test]
    fn fails_to_delete_with_a_stale_read() {
        let store = RefCell::new(Store::new());
        let mut cache: Cache<String, _> = Cache::new(&store);

        assert_eq!(cache.write("x", 'a'), true);

        assert_eq!(store.borrow_mut().write("x", Some(1), 'c'), Some(2));
        assert_eq!(cache.remove("x"), false);

        assert_eq!(store.borrow().read("x"), Some((2, 'c')));
    }

    #[test]
    fn recovers_after_a_failed_write() {
        let store = RefCell::new(Store::new());
        let mut cache: Cache<String, _> = Cache::new(&store);

        assert_eq!(cache.write("x", 'a'), true);

        assert_eq!(store.borrow_mut().write("x", Some(1), 'c'), Some(2));
        assert_eq!(cache.write("x", 'b'), false);

        assert_eq!(cache.read("x"), Some('c'));
        assert_eq!(cache.write("x", 'b'), true);

        assert_eq!(store.borrow().read("x"), Some((3, 'b')));
        assert_eq!(cache.read("x"), Some('b'));
    }

    #[test]
    fn allows_multiple_clients_to_mutate_the_store() {
        let store = RefCell::new(Store::new());
        let mut a: Cache<String, _> = Cache::new(&store);
        let mut b: Cache<String, _> = Cache::new(&store);

        assert_eq!(a.write("x", 'a'), true);
        assert_eq!(b.write("y", 'b'), true);

        assert_eq!(a.write("y", 'a'), false);
        assert_eq!(b.write("x", 'b'), false);

        assert_eq!(a.read("y"), Some('b'));
        assert_eq!(b.read("x"), Some('a'));
    }
}
