package bifrore.router.server;

import bifrore.map.store.AbstractPersistentStore;
import org.rocksdb.RocksDBException;

public class IdMapStore extends AbstractPersistentStore<String, byte[]> {
    public IdMapStore(String dbPath) throws RocksDBException {
        super(dbPath, String::getBytes, String::new, s -> s, s -> s);
    }
}
