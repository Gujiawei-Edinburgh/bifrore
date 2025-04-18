package bifrore.baserpc.discovery;

import bifrore.baserpc.ClusterMemberListener;
import bifrore.baserpc.IClusterManager;
import com.google.common.collect.Sets;
import io.reactivex.rxjava3.core.Observable;
import io.reactivex.rxjava3.subjects.BehaviorSubject;

import java.util.Set;
import java.util.UUID;

public class TrafficDiscoveryImpl implements ITrafficDiscovery {
    private final BehaviorSubject<Set<Server>> serverListSubject = BehaviorSubject.create();
    private final UUID listenerId;
    private final IClusterManager clusterManager;

    public TrafficDiscoveryImpl(IClusterManager clusterManager) {
        this.clusterManager = clusterManager;
        ClusterMemberListener listener = new ClusterMemberListener() {

            @Override
            public void onMemberJoin(Set<String> members) {
                Set<Server> servers = Sets.newHashSet();
                for (String serverInfo : members) {
                    servers.add(new Server(serverInfo));
                }
                serverListSubject.onNext(servers);
            }

            @Override
            public void onMemberLeave(Set<String> members) {
                Set<Server> servers = Sets.newHashSet();
                for (String serverInfo : members) {
                    servers.add(new Server(serverInfo));
                }
                serverListSubject.onNext(servers);
            }
        };
        this.listenerId = clusterManager.addClusterMemberListener(listener);
    }

    public Observable<Set<Server>> start() {
        return this.serverListSubject;
    }

    public void stop() {
        this.clusterManager.removeClusterMemberListener(this.listenerId);
        this.serverListSubject.onComplete();
    }
}
