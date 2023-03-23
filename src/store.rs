use std::collections::BTreeMap;

type Rev = usize;

#[derive(Clone)]
pub struct Store<T> {
    data: BTreeMap<String, (Rev, Option<T>)>,
    pub seq: Rev,
}

impl<T> Store<T>
where
    T: Clone,
{
    pub fn new() -> Store<T> {
        Store {
            data: BTreeMap::new(),
            seq: 0,
        }
    }

    pub fn read(&self, key: &str) -> Option<(Rev, T)> {
        if let Some((rev, Some(value))) = self.data.get(key) {
            Some((*rev, value.clone()))
        } else {
            None
        }
    }

    pub fn write(&mut self, key: &str, rev: Option<Rev>, value: T) -> Option<Rev> {
        self.set_key(key, rev, Some(value))
    }

    fn set_key(&mut self, key: &str, rev: Option<Rev>, value: Option<T>) -> Option<Rev> {
        let client_rev = rev.unwrap_or(0);
        let entry = self.data.entry(key.into()).or_insert((0, None));

        if entry.0 != client_rev {
            return None;
        }

        *entry = (entry.0 + 1, value);
        self.seq += 1;

        Some(entry.0)
    }

    pub fn keys(&self) -> Vec<&str> {
        self.data.keys().map(|key| key.as_ref()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_for_unknown_key() {
        let store: Store<()> = Store::new();
        assert_eq!(store.seq, 0);
        assert_eq!(store.read("x"), None);
    }

    #[test]
    fn stores_a_new_value() {
        let mut store = Store::new();
        assert_eq!(store.write("x", None, 51), Some(1));
        assert_eq!(store.seq, 1);
        assert_eq!(store.read("x"), Some((1, 51)));
    }

    #[test]
    fn does_not_update_a_value_without_a_rev() {
        let mut store = Store::new();
        store.write("x", None, 51);

        assert_eq!(store.write("x", None, 52), None);
        assert_eq!(store.seq, 1);
        assert_eq!(store.read("x"), Some((1, 51)));
    }

    #[test]
    fn does_not_update_a_value_with_a_bad_rev() {
        let mut store = Store::new();
        let rev = store.write("x", None, 51).unwrap();

        assert_eq!(store.write("x", Some(rev + 1), 52), None);
        assert_eq!(store.seq, 1);
        assert_eq!(store.read("x"), Some((1, 51)));
    }

    #[test]
    fn updates_a_value_with_a_matching_rev() {
        let mut store = Store::new();
        let rev = store.write("x", None, 51).unwrap();

        assert_eq!(store.write("x", Some(rev), 52), Some(2));
        assert_eq!(store.seq, 2);
        assert_eq!(store.read("x"), Some((2, 52)));
    }

    #[test]
    fn returns_all_the_keys_in_the_store() {
        let mut store = Store::new();

        store.write("/", None, 51);
        store.write("/path/", None, 52);
        store.write("/z/doc.json", None, 53);

        assert_eq!(store.keys(), vec!["/", "/path/", "/z/doc.json"]);
    }
}
