package bifrore.baserpc;

import com.hazelcast.collection.ISet;

public class ClusterManagerBuilder {
    ISet<String> servers;

    public ClusterManagerBuilder servers(ISet<String> servers) {
        this.servers = servers;
        return this;
    }

    public IClusterManager build() {
        return new ClusterManager(this.servers);
    }
}
