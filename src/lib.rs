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
use parking_lot::RwLock;
use std::{
    collections::hash_map::{HashMap, RandomState},
    hash::{BuildHasher, Hash, Hasher},
    ops::Deref,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

pub struct FixedSizeLruMap<K, V, S = RandomState> {
    age: AtomicU64,
    capacity: usize,
    map: RwLock<HashMap<K, MapGuard<V>, S>>,
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
            age: AtomicU64::new(0),
            capacity: capacity,
            map: RwLock::from(HashMap::with_capacity_and_hasher(
                capacity + 1,
                hash_builder,
            )),
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.map.read().contains_key(key)
    }

    pub fn get(&self, key: &K) -> Option<MapGuard<V>> {
        let map = self.map.read();
        let guard = map.get(key)?;
        self.update_guard_age(guard);
        Some(MapGuard::clone(&guard))
    }

    pub fn get_or_init<F>(&self, key: K, f: F) -> MapGuard<V>
    where
        F: FnOnce() -> V,
        K: Clone,
    {
        match self.get(&key) {
            Some(value) => value,
            None => self.insert(key, f()).0,
        }
    }

    pub fn insert(&self, key: K, value: V) -> (MapGuard<V>, Option<MapGuard<V>>)
    where
        K: Clone,
    {
        let mut map = self.map.write();
        let age = self.age.fetch_add(1, Ordering::SeqCst);
        let guard = MapGuard(Arc::new((AtomicU64::new(age), value)));
        let mut old = map.insert(key, guard.clone());

        if old.is_none() && map.len() > self.capacity {
            if let Some(key) = map
                .iter()
                .min_by_key(|(_, v)| v.age())
                .map(|(k, _)| k.clone())
            {
                old = map.remove(&key);
            }
        }

        (guard, old)
    }

    pub fn is_empty(&self) -> bool {
        self.map.read().is_empty()
    }

    pub fn len(&self) -> usize {
        self.map.read().len()
    }

    pub fn remove(&self, key: &K) -> Option<MapGuard<V>> {
        self.map.write().remove(key)
    }

    fn update_guard_age(&self, guard: &MapGuard<V>) {
        let v = self.age.fetch_add(1, Ordering::SeqCst);
        guard.set_age(v);
    }
}

pub struct MapGuard<V>(Arc<(AtomicU64, V)>);

impl<V> MapGuard<V> {
    fn age(&self) -> u64 {
        (self.0).0.load(Ordering::Relaxed)
    }

    fn set_age(&self, value: u64) {
        (self.0).0.store(value, Ordering::Relaxed);
    }

    pub fn try_unwrap(this: MapGuard<V>) -> Result<V, MapGuard<V>> {
        match Arc::try_unwrap(this.0) {
            Ok(inner) => Ok(inner.1),
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
        &(self.0).1
    }
}

impl<V> Eq for MapGuard<V> where V: Eq {}

impl<V> Hash for MapGuard<V>
where
    V: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0).1.hash(state)
    }
}

impl<V> Ord for MapGuard<V>
where
    V: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.0).1.cmp(&(other.0).1)
    }
}

impl<V> PartialEq for MapGuard<V>
where
    V: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        (self.0).1 == (other.0).1
    }
}

impl<V> PartialOrd for MapGuard<V>
where
    V: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        (self.0).1.partial_cmp(&(other.0).1)
    }
}
