//! Map replicating JavaScript object property-key ordering.
//!
//! JS objects iterate array-index-like keys first in ascending numeric order,
//! then all other string keys in insertion order. dagre's tie-breaking
//! behavior depends on this ordering, so the port must reproduce it exactly.

use indexmap::IndexMap;

/// Returns the array-index value of `key` if it is array-index-like.
///
/// A key is array-index-like when it is the canonical decimal form of an
/// integer in `0..=u32::MAX - 1` (no leading zeros, no sign, no fraction).
fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || key.len() > 10 {
        return None;
    }
    if !key.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if key.len() > 1 && key.starts_with('0') {
        return None;
    }
    let n: u64 = key.parse().ok()?;
    if n < u64::from(u32::MAX) {
        Some(u32::try_from(n).expect("value below u32::MAX"))
    } else {
        None
    }
}

/// Insertion-ordered string map with JS object key-iteration semantics.
#[derive(Debug, Clone)]
pub struct JsMap<V> {
    map: IndexMap<String, V>,
    /// Number of array-index-like keys currently present. When zero (the
    /// common case for dagre node names), iteration is plain insertion order.
    index_keys: usize,
}

impl<V> Default for JsMap<V> {
    fn default() -> Self {
        Self {
            map: IndexMap::new(),
            index_keys: 0,
        }
    }
}

impl<V> JsMap<V> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&V> {
        self.map.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.map.get_mut(key)
    }

    /// Inserts `value` under `key`. An existing key keeps its position, as in JS.
    pub fn insert(&mut self, key: impl Into<String>, value: V) -> Option<V> {
        let key = key.into();
        let is_index = array_index(&key).is_some();
        let prev = self.map.insert(key, value);
        if prev.is_none() && is_index {
            self.index_keys += 1;
        }
        prev
    }

    /// Removes `key`, preserving the relative order of the remaining keys.
    pub fn remove(&mut self, key: &str) -> Option<V> {
        let removed = self.map.shift_remove(key);
        if removed.is_some() && array_index(key).is_some() {
            self.index_keys -= 1;
        }
        removed
    }

    /// Keys in JS object iteration order.
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        if self.index_keys == 0 {
            return self.map.keys().cloned().collect();
        }
        let mut numeric: Vec<(u32, &String)> = Vec::with_capacity(self.index_keys);
        let mut rest: Vec<&String> = Vec::new();
        for key in self.map.keys() {
            match array_index(key) {
                Some(n) => numeric.push((n, key)),
                None => rest.push(key),
            }
        }
        numeric.sort_by_key(|&(n, _)| n);
        numeric
            .into_iter()
            .map(|(_, k)| k.clone())
            .chain(rest.into_iter().cloned())
            .collect()
    }

    /// Values in JS object iteration order.
    #[must_use]
    pub fn values(&self) -> Vec<&V> {
        if self.index_keys == 0 {
            return self.map.values().collect();
        }
        self.keys()
            .iter()
            .map(|k| self.map.get(k).expect("key from keys()"))
            .collect()
    }

    /// `(key, value)` pairs in JS object iteration order.
    #[must_use]
    pub fn entries(&self) -> Vec<(String, &V)> {
        self.keys()
            .into_iter()
            .map(|k| {
                let v = self.map.get(&k).expect("key from keys()");
                (k, v)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertion_order_for_plain_keys() {
        let mut m = JsMap::new();
        m.insert("b", 1);
        m.insert("a", 2);
        m.insert("_d10", 3);
        assert_eq!(m.keys(), vec!["b", "a", "_d10"]);
    }

    #[test]
    fn numeric_keys_first_ascending() {
        let mut m = JsMap::new();
        m.insert("x", 0);
        m.insert("10", 0);
        m.insert("2", 0);
        m.insert("a", 0);
        assert_eq!(m.keys(), vec!["2", "10", "x", "a"]);
    }

    #[test]
    fn non_canonical_numerics_are_string_keys() {
        let mut m = JsMap::new();
        m.insert("01", 0);
        m.insert("1.5", 0);
        m.insert("1", 0);
        assert_eq!(m.keys(), vec!["1", "01", "1.5"]);
    }

    #[test]
    fn reinsert_after_remove_moves_to_end() {
        let mut m = JsMap::new();
        m.insert("a", 1);
        m.insert("b", 2);
        m.insert("c", 3);
        m.remove("a");
        m.insert("a", 4);
        assert_eq!(m.keys(), vec!["b", "c", "a"]);
    }

    #[test]
    fn overwrite_keeps_position() {
        let mut m = JsMap::new();
        m.insert("a", 1);
        m.insert("b", 2);
        m.insert("a", 3);
        assert_eq!(m.keys(), vec!["a", "b"]);
        assert_eq!(m.get("a"), Some(&3));
    }
}
