package bifromq.re.baserpc.discovery;

import com.google.common.collect.Sets;
import com.hazelcast.collection.ISet;
import com.hazelcast.collection.ItemEvent;
import com.hazelcast.collection.ItemListener;
import io.reactivex.rxjava3.core.Observable;
import io.reactivex.rxjava3.subjects.BehaviorSubject;

import java.util.Set;
import java.util.UUID;

public class TrafficDiscoveryImpl implements ITrafficDiscovery {
    private final ISet<String> serverSet;
    private final BehaviorSubject<Set<Server>> serverListSubject = BehaviorSubject.create();
    private final UUID listerId;

    class ServerChangeListener implements ItemListener<String> {
        @Override
        public void itemAdded(ItemEvent<String> itemEvent) {
            Set<Server> servers = Sets.newHashSet();
            for (String serverInfo : serverSet) {
                servers.add(new Server(serverInfo));
            }
            serverListSubject.onNext(servers);
        }

        @Override
        public void itemRemoved(ItemEvent<String> itemEvent) {
            Set<Server> servers = Sets.newHashSet();
            for (String serverInfo : serverSet) {
                servers.add(new Server(serverInfo));
            }
            serverListSubject.onNext(servers);
        }
    }

    public TrafficDiscoveryImpl(String serviceUniqueName) {
        this.serverSet = TrafficProvider.getInstance().getSet(serviceUniqueName);
        this.listerId = this.serverSet.addItemListener(new ServerChangeListener(), true);
    }

    public Observable<Set<Server>> start() {
        return this.serverListSubject;
    }

    public void stop() {
        this.serverSet.removeItemListener(this.listerId);
        this.serverListSubject.onComplete();
    }
}
