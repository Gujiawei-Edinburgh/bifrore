package bifrore.router.server.store;

import org.rocksdb.RocksDBException;

public class IdMapStore extends RocksDBMapStore<String, byte[]>{
    public IdMapStore(String dbPath) throws RocksDBException {
        super(dbPath, String::getBytes, s -> s, s -> s);
    }
}
