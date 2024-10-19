package bifromq.re.baserpc;

import com.hazelcast.collection.ISet;
import com.hazelcast.collection.ItemEvent;
import com.hazelcast.collection.ItemListener;
import lombok.extern.slf4j.Slf4j;

import java.util.Map;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

@Slf4j
final class ClusterManager implements IClusterManager{
    private final ISet<String> servers;
    private final Map<UUID, ClusterMemberListener> listenerMap = new ConcurrentHashMap<>();
    private final UUID listenerId;

    ClusterManager(ISet<String> servers) {
        this.servers = servers;
        listenerId = servers.addItemListener(new ItemListener<>() {
            @Override
            public void itemAdded(ItemEvent<String> item) {
                log.info("Cluster member joined: {}", item.getItem());
                listenerMap.values().forEach(l -> l.onMemberJoin(servers));
            }

            @Override
            public void itemRemoved(ItemEvent<String> item) {
                log.info("Cluster member left: {}", item.getItem());
                listenerMap.values().forEach(l -> l.onMemberLeave(servers));
            }
        }, true);
    }

    @Override
    public void addClusterMember(String clusterMember) {
        servers.add(clusterMember);
    }

    @Override
    public void removeClusterMember(String clusterMember) {
        servers.remove(clusterMember);
    }

    @Override
    public UUID addClusterMemberListener(ClusterMemberListener listener) {
        UUID uuid = UUID.randomUUID();
        listenerMap.put(uuid, listener);
        return uuid;
    }

    @Override
    public void removeClusterMemberListener(UUID listenerId) {
        listenerMap.remove(listenerId);
    }

    @Override
    public void close() {
        servers.removeItemListener(listenerId);
    }
}
