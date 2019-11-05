use crate::errors::*;
use bitcoin_hashes::sha256d::Hash as Sha256dHash;
use lru::LruCache;
use std::hash::Hash;
use std::sync::Mutex;


//
// LRU cache with a fixed size
//
struct SizedLruCache<K, V> {
    map: LruCache<K, (V, usize)>,
    bytes_usage: usize,
    bytes_capacity: usize,
}

impl<K: Hash + Eq, V> SizedLruCache<K, V> {
    fn new(bytes_capacity: usize) -> SizedLruCache<K, V> {
        SizedLruCache {
            map: LruCache::unbounded(),
            bytes_usage: 0,
            bytes_capacity,
        }
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        match self.map.get(key) {
            None => None,
            Some((value, _)) => Some(value)
        }
    }

    fn put(&mut self, key: K, value: V, byte_size: usize) {
        if byte_size > self.bytes_capacity {
            return;
        }
        if let Some((_, popped_size)) = self.map.put(key, (value, byte_size)) {
            self.bytes_usage -= popped_size
        }
        self.bytes_usage += byte_size;

        while self.bytes_usage > self.bytes_capacity {
            match self.map.pop_lru() {
                Some((_, (_, popped_size))) => self.bytes_usage -= popped_size,
                None => return,
            }
        }
    }
}

//
// Cache storing the txids of transactions included in a block
//
pub struct BlockTxIDsCache {
    map: Mutex<SizedLruCache<Sha256dHash /* blockhash */, Vec<Sha256dHash /* txid */>>>,
}

impl BlockTxIDsCache {
    pub fn new(bytes_capacity: usize) -> BlockTxIDsCache {
        BlockTxIDsCache {
            map: Mutex::new(SizedLruCache::new(bytes_capacity)),
        }
    }

    pub fn get_or_else<F>(
        &self,
        blockhash: &Sha256dHash,
        load_txids_func: F,
    ) -> Result<Vec<Sha256dHash>>
    where
        F: FnOnce() -> Result<Vec<Sha256dHash>>,
    {
        if let Some(txids) = self.map.lock().unwrap().get(blockhash) {
            return Ok(txids.clone());
        }

        let txids = load_txids_func()?;
        let byte_size = 32 /* hash size */ * (1 /* key */ + txids.len() /* values */);
        self.map
            .lock()
            .unwrap()
            .put(*blockhash, txids.clone(), byte_size);
        Ok(txids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin_hashes::Hash;

    #[test]
    fn test_sized_lru_cache_hit_and_miss() {
        let mut cache = SizedLruCache::<i8, i32>::new(100,);

        assert_eq!(cache.get(&1), None); // no such key

        cache.put(1, 10, 50); // add new key-value
        assert_eq!(cache.get(&1), Some(&10));

        cache.put(3, 30, 50); // drop oldest key (1)
        cache.put(2, 20, 50);
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), Some(&20));
        assert_eq!(cache.get(&3), Some(&30));

        cache.put(3, 33, 50); // replace existing value
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), Some(&20));
        assert_eq!(cache.get(&3), Some(&33));

        cache.put(9, 90, 9999); // larger than cache capacity, don't drop the cache
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), Some(&20));
        assert_eq!(cache.get(&3), Some(&33));
        assert_eq!(cache.get(&9), None);
    }

    fn gen_hash(seed: u8) -> Sha256dHash {
        let bytes: Vec<u8> = (seed..seed + 32).collect();
        Sha256dHash::hash(&bytes[..])
    }

    #[test]
    fn test_blocktxids_cache_hit_and_miss() {
        let block1 = gen_hash(1);
        let block2 = gen_hash(2);
        let block3 = gen_hash(3);
        let txids = vec![gen_hash(4), gen_hash(5)];

        let misses: Mutex<usize> = Mutex::new(0);
        let miss_func = || {
            *misses.lock().unwrap() += 1;
            Ok(txids.clone())
        };

        // 200 bytes ~ 32 (bytes/hash) * (1 key hash + 2 value hashes) * 2 txns
        let cache = BlockTxIDsCache::new(200);

        // cache miss
        let result = cache.get_or_else(&block1, &miss_func).unwrap();
        assert_eq!(1, *misses.lock().unwrap());
        assert_eq!(txids, result);

        // cache hit
        let result = cache.get_or_else(&block1, &miss_func).unwrap();
        assert_eq!(1, *misses.lock().unwrap());
        assert_eq!(txids, result);

        // cache size is 200, test that blockhash1 falls out of cache
        cache.get_or_else(&block2, &miss_func).unwrap();
        assert_eq!(2, *misses.lock().unwrap());
        cache.get_or_else(&block3, &miss_func).unwrap();
        assert_eq!(3, *misses.lock().unwrap());
        cache.get_or_else(&block1, &miss_func).unwrap();
        assert_eq!(4, *misses.lock().unwrap());

        // cache hits
        cache.get_or_else(&block3, &miss_func).unwrap();
        cache.get_or_else(&block1, &miss_func).unwrap();
        assert_eq!(4, *misses.lock().unwrap());
    }
}
