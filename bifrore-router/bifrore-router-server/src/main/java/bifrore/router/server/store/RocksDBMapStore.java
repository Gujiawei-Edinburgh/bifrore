package bifrore.router.server.store;

import com.hazelcast.map.MapLoader;
import com.hazelcast.map.MapStore;
import org.rocksdb.*;
import io.micrometer.core.instrument.Metrics;
import java.util.Collection;
import java.util.HashMap;
import java.util.Map;
import java.util.function.Function;

abstract class RocksDBMapStore<K, V> implements MapStore<K, V>, MapLoader<K, V> {
    private final RocksDB rocksDB;
    private final Function<K, byte[]> keySerializer;
    private final Function<V, byte[]> valueSerializer;
    private final Function<byte[], V> valueDeserializer;

    public RocksDBMapStore(String dbPath,
                           Function<K, byte[]> keySerializer,
                           Function<V, byte[]> valueSerializer,
                           Function<byte[], V> valueDeserializer) throws RocksDBException {
        RocksDB.loadLibrary();
        Options options = new Options().setCreateIfMissing(true);
        Statistics stat = new Statistics();
        options.setStatistics(stat);
        Metrics.gauge("rocksdb.block_cache_hits", stat.getTickerCount(TickerType.BLOCK_CACHE_HIT));
        Metrics.gauge("rocksdb.block_cache_misses", stat.getTickerCount(TickerType.BLOCK_CACHE_MISS));
        this.rocksDB = RocksDB.open(options, dbPath);
        this.keySerializer = keySerializer;
        this.valueSerializer = valueSerializer;
        this.valueDeserializer = valueDeserializer;
    }

    @Override
    public void store(K k, V v) {
        try {
            rocksDB.put(keySerializer.apply(k), valueSerializer.apply(v));
        } catch (RocksDBException e) {
            throw new RuntimeException("Failed to store data in RocksDB", e);
        }
    }

    @Override
    public void storeAll(Map<K, V> map) {
        try (WriteBatch batch = new WriteBatch();
             WriteOptions options = new WriteOptions()) {
            for (Map.Entry<K, V> entry : map.entrySet()) {
                batch.put(keySerializer.apply(entry.getKey()), valueSerializer.apply(entry.getValue()));
            }
            rocksDB.write(options, batch);
        } catch (RocksDBException e) {
            throw new RuntimeException("Failed to store multiple entries in RocksDB", e);
        }
    }

    @Override
    public void delete(K k) {
        try {
            rocksDB.delete(keySerializer.apply(k));
        } catch (RocksDBException e) {
            throw new RuntimeException("Failed to delete data from RocksDB", e);
        }
    }

    @Override
    public void deleteAll(Collection<K> keys) {
        for (K key : keys) {
            delete(key);
        }
    }

    @Override
    public V load(K k) {
        try {
            byte[] valueBytes = rocksDB.get(keySerializer.apply(k));
            return valueDeserializer.apply(valueBytes);
        } catch (RocksDBException e) {
            throw new RuntimeException("Failed to load data from RocksDB", e);
        }
    }

    @Override
    public Map<K, V> loadAll(Collection<K> keys) {
        Map<K, V> map = new HashMap<>();
        for (K key : keys) {
            V value = load(key);
            if (value != null) {
                map.put(key, value);
            }
        }
        return map;
    }

    @Override
    public Iterable<K> loadAllKeys() {
        return null;
    }
}
