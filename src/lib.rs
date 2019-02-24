//! A fixed size map that keeps only the most recently used values.
//! This can be used like a cache and it is thread safe.
//!
//! # Example
//! ```
//! use fixed_size_lru_map::FixedSizeLruMap;
//!
//! fn main() {
//!     let map = FixedSizeLruMap::with_capacity(2);
//!     let a = *map.get_or_init("a", || 10);
//!     let b = *map.get_or_init("a", || 12);
//!
//!     assert_eq!(10, a);
//!     assert_eq!(10, b);
//!     assert_eq!(1, map.len());
//! }
//! ```
use arc_swap::ArcSwap;
use std::collections::hash_map::{HashMap, RandomState};
use std::hash::{BuildHasher, Hash, Hasher};
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct FixedSizeLruMap<K, V, S = RandomState> {
    age: AtomicUsize,
    capacity: usize,
    map: ArcSwap<HashMap<K, MapGuard<V>, S>>,
}

impl<K, V> FixedSizeLruMap<K, V>
where
    K: Eq + Hash,
{
    pub fn with_capacity(capacity: usize) -> FixedSizeLruMap<K, V> {
        Self::with_capacity_and_hasher(capacity, Default::default())
    }
}

impl<K, V, S> FixedSizeLruMap<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        FixedSizeLruMap {
            age: AtomicUsize::new(0),
            capacity: capacity,
            map: ArcSwap::from(Arc::new(HashMap::with_capacity_and_hasher(
                capacity + 1,
                hash_builder,
            ))),
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.map.lease().contains_key(key)
    }

    pub fn get(&self, key: &K) -> Option<MapGuard<V>> {
        match self.map.lease().get(key) {
            Some(guard) => {
                self.update_guard_age(guard);
                Some(MapGuard::clone(&guard))
            }
            None => None,
        }
    }

    pub fn get_or_init<F>(&self, key: K, f: F) -> MapGuard<V>
    where
        F: FnOnce() -> V,
        K: Clone,
        S: Clone,
    {
        match self.get(&key) {
            Some(value) => value,
            None => self.insert(key, f()).0,
        }
    }

    pub fn insert(&self, key: K, value: V) -> (MapGuard<V>, Option<MapGuard<V>>)
    where
        K: Clone,
        S: Clone,
    {
        let age = self.age.fetch_add(1, Ordering::SeqCst);

        let guard = MapGuard(Arc::new(Inner {
            age: AtomicUsize::new(age),
            value,
        }));

        let mut old = None;

        self.map.rcu(|map| {
            let mut map = HashMap::clone(&map);
            old = map.insert(key.clone(), guard.clone());

            if old.is_none() && map.len() > self.capacity {
                // find the last used key
                let key = map
                    .iter()
                    .min_by(|l, r| (l.1).0.age().cmp(&(r.1).0.age()))
                    .map(|t| t.0)
                    .cloned();

                if let Some(key) = key {
                    map.remove(&key);
                }
            }

            map
        });

        (guard, old)
    }

    pub fn is_empty(&self) -> bool {
        self.map.lease().is_empty()
    }

    pub fn len(&self) -> usize {
        self.map.lease().len()
    }

    pub fn remove(&self, key: &K) -> Option<MapGuard<V>>
    where
        K: Clone,
        S: Clone,
    {
        if !self.contains_key(key) {
            return None;
        }

        let mut value = None;

        self.map.rcu(|map| {
            let mut map = HashMap::clone(&map);
            value = map.remove(key);
            map
        });

        value
    }

    fn update_guard_age(&self, guard: &MapGuard<V>) {
        let v = self.age.fetch_add(1, Ordering::SeqCst);
        guard.0.age.swap(v, Ordering::Relaxed);
    }
}

pub struct MapGuard<V>(Arc<Inner<V>>);

impl<V> MapGuard<V> {
    pub fn try_unwrap(this: MapGuard<V>) -> Result<V, MapGuard<V>> {
        match Arc::try_unwrap(this.0) {
            Ok(inner) => Ok(inner.value),
            Err(arc) => Err(MapGuard(arc)),
        }
    }
}

impl<V> Clone for MapGuard<V> {
    fn clone(&self) -> Self {
        MapGuard(Arc::clone(&self.0))
    }
}

impl<V> Deref for MapGuard<V> {
    type Target = V;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0.value
    }
}

impl<V> Eq for MapGuard<V> where V: Eq {}

impl<V> Hash for MapGuard<V>
where
    V: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.value.hash(state)
    }
}

impl<V> Ord for MapGuard<V>
where
    V: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.value.cmp(&other.0.value)
    }
}

impl<V> PartialEq for MapGuard<V>
where
    V: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.0.value == other.0.value
    }
}

impl<V> PartialOrd for MapGuard<V>
where
    V: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.value.partial_cmp(&other.0.value)
    }
}

struct Inner<V> {
    age: AtomicUsize,
    value: V,
}

impl<V> Inner<V> {
    fn age(&self) -> usize {
        self.age.load(Ordering::Relaxed)
    }
}
