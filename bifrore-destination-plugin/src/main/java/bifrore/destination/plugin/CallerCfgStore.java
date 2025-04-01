package bifrore.destination.plugin;

import bifrore.map.store.AbstractPersistentStore;
import org.rocksdb.RocksDBException;

public class CallerCfgStore extends AbstractPersistentStore<String, byte[]> {
    public CallerCfgStore(String dbPath) throws RocksDBException {
        super(dbPath, String::getBytes, String::new, s -> s, s -> s);
    }
}
