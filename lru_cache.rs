use std::collections::HashMap;
use std::hash::Hash;

/// A node in the doubly-linked list
struct Node<V> {
    value: V,
    prev: usize,  // index of previous node, 0 if none (sentinel)
    next: usize,  // index of next node, 0 if none (sentinel)
}

/// LRU Cache implementation using HashMap and doubly-linked list
/// Uses sentinel nodes for simpler edge case handling
pub struct LruCache<K, V> {
    capacity: usize,
    cache: HashMap<K, usize>, // key -> data node index
    nodes: Vec<Node<V>>,      // index 0 and 1 are sentinels
    key_pool: Vec<Option<K>>, // key pool indexed by node index
    free_keys: Vec<usize>,    // free key indices
}

impl<K, V> LruCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Creates a new LRU cache with the given capacity
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Capacity must be greater than 0");
        
        let cap_plus_2 = capacity + 2;
        let mut nodes = Vec::with_capacity(cap_plus_2);
        let mut key_pool = Vec::with_capacity(cap_plus_2);
        
        // Sentinel nodes: 0 = head (most recent), 1 = tail (least recent)
        nodes.push(Node {
            value: unsafe { std::mem::zeroed() },
            prev: 0,
            next: 1,
        });
        nodes.push(Node {
            value: unsafe { std::mem::zeroed() },
            prev: 0,
            next: 1,
        });
        
        for _ in 0..capacity {
            key_pool.push(None);
        }
        
        LruCache {
            capacity,
            cache: HashMap::new(),
            nodes,
            key_pool,
            free_keys: (0..capacity).collect(),
        }
    }

    /// Allocates a new key index
    fn alloc_key(&mut self) -> usize {
        self.free_keys.pop().unwrap()
    }

    /// Frees a key index
    fn free_key(&mut self, idx: usize) {
        self.key_pool[idx] = None;
        self.free_keys.push(idx);
    }

    /// Creates a new data node at the given index
    fn make_data_node(&mut self, idx: usize, key: K, value: V) {
        self.key_pool[idx] = Some(key);
        self.nodes[idx] = Node {
            value,
            prev: 0,
            next: 1,
        };
    }

    /// Removes a node from the linked list
    fn unlink(&mut self, idx: usize) {
        let prev = self.nodes[idx].prev;
        let next = self.nodes[idx].next;
        
        self.nodes[prev].next = next;
        self.nodes[next].prev = prev;
    }

    /// Inserts a node right after the head (most recently used)
    fn push_front(&mut self, idx: usize) {
        let head_next = self.nodes[0].next;
        
        self.nodes[idx].prev = 0;
        self.nodes[idx].next = head_next;
        
        self.nodes[0].next = idx;
        self.nodes[head_next].prev = idx;
    }

    /// Removes the least recently used node (before tail)
    fn pop_back(&mut self) -> usize {
        let tail_prev = self.nodes[1].prev;
        self.unlink(tail_prev);
        tail_prev
    }

    /// Gets the value for the given key, returning None if not found.
    /// Moves the accessed node to the front (most recently used).
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let &node_idx = self.cache.get(key)?;
        
        // Unlink from current position
        self.unlink(node_idx);
        
        // Re-link at front
        self.push_front(node_idx);
        
        let node = &self.nodes[node_idx];
        Some(&node.value)
    }

    /// Inserts or updates a key-value pair in the cache.
    /// Evicts the least recently used item if capacity is exceeded.
    pub fn put(&mut self, key: K, value: V)
    where
        K: Clone,
    {
        if let Some(&node_idx) = self.cache.get(&key) {
            // Update existing value
            self.nodes[node_idx].value = value;
            
            // Move to front
            self.unlink(node_idx);
            self.push_front(node_idx);
        } else {
            // Evict if at capacity
            if self.cache.len() >= self.capacity {
                self.evict();
            }
            
            // Create new node
            let node_idx = self.alloc_key();
            self.make_data_node(node_idx, key.clone(), value);
            
            // Insert into cache
            self.cache.insert(key, node_idx);
            
            // Push to front
            self.push_front(node_idx);
        }
    }

    /// Evicts the least recently used item
    fn evict(&mut self) {
        let node_idx = self.pop_back();
        if let Some(key) = self.key_pool[node_idx].take() {
            self.cache.remove(&key);
        }
        self.free_key(node_idx);
    }
}

impl<K, V> Drop for LruCache<K, V> {
    fn drop(&mut self) {
        // Clear all data
        self.cache.clear();
        self.nodes.clear();
        self.key_pool.clear();
        self.free_keys.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_put_and_get() {
        let mut cache = LruCache::new(2);
        cache.put("key1", "value1");
        cache.put("key2", "value2");

        assert_eq!(cache.get(&"key1"), Some(&"value1"));
        assert_eq!(cache.get(&"key2"), Some(&"value2"));
    }

    #[test]
    fn test_update_existing_key() {
        let mut cache = LruCache::new(2);
        cache.put("key1", "value1");
        cache.put("key1", "value1_updated");

        assert_eq!(cache.get(&"key1"), Some(&"value1_updated"));
    }

    #[test]
    fn test_eviction_lru() {
        let mut cache = LruCache::new(2);
        cache.put("key1", "value1");
        cache.put("key2", "value2");

        // Access key1 to make it recently used
        cache.get(&"key1");

        // Add key3, should evict key2 (least recently used)
        cache.put("key3", "value3");

        assert_eq!(cache.get(&"key1"), Some(&"value1"));
        assert_eq!(cache.get(&"key2"), None); // Evicted
        assert_eq!(cache.get(&"key3"), Some(&"value3"));
    }

    #[test]
    fn test_eviction_order() {
        let mut cache = LruCache::new(3);
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Access 'a' to make it recently used
        cache.get(&"a");

        // Access 'b' to make it recently used
        cache.get(&"b");

        // Add 'd', should evict 'c' (least recently used)
        cache.put("d", 4);

        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"b"), Some(&2));
        assert_eq!(cache.get(&"c"), None); // Evicted
        assert_eq!(cache.get(&"d"), Some(&4));
    }

    #[test]
    fn test_single_capacity() {
        let mut cache = LruCache::new(1);
        cache.put("key1", "value1");
        cache.put("key2", "value2");

        assert_eq!(cache.get(&"key1"), None); // Evicted
        assert_eq!(cache.get(&"key2"), Some(&"value2"));
    }

    #[test]
    fn test_eviction_with_put_on_existing() {
        let mut cache = LruCache::new(2);
        cache.put("key1", "value1");
        cache.put("key2", "value2");

        // Update key1 - this should make it most recently used
        cache.put("key1", "value1_updated");

        // Add key3 - should evict key2 (LRU)
        cache.put("key3", "value3");

        assert_eq!(cache.get(&"key1"), Some(&"value1_updated"));
        assert_eq!(cache.get(&"key2"), None); // Evicted
        assert_eq!(cache.get(&"key3"), Some(&"value3"));
    }

    #[test]
    fn test_nonexistent_key() {
        let mut cache = LruCache::new(2);
        assert_eq!(cache.get(&"nonexistent"), None);
    }

    #[test]
    fn test_multiple_evictions() {
        let mut cache = LruCache::new(3);
        cache.put(1, "one");
        cache.put(2, "two");
        cache.put(3, "three");

        // Access 1, making it recently used
        cache.get(&1);

        // Add 4, evicts 2 (LRU)
        cache.put(4, "four");

        assert_eq!(cache.get(&1), Some(&"one"));
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&3), Some(&"three"));
        assert_eq!(cache.get(&4), Some(&"four"));

        // Access 3
        cache.get(&3);

        // Add 5, evicts 4 (now LRU)
        cache.put(5, "five");

        assert_eq!(cache.get(&1), Some(&"one"));
        assert_eq!(cache.get(&3), Some(&"three"));
        assert_eq!(cache.get(&4), None);
        assert_eq!(cache.get(&5), Some(&"five"));
    }

    #[test]
    fn test_integer_keys() {
        let mut cache = LruCache::new(3);
        cache.put(100, "value100");
        cache.put(200, "value200");
        cache.put(300, "value300");

        assert_eq!(cache.get(&100), Some(&"value100"));
        assert_eq!(cache.get(&200), Some(&"value200"));
        assert_eq!(cache.get(&300), Some(&"value300"));
    }
}
