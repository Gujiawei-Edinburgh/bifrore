package bifromq.re.baserpc.discovery;

import bifromq.re.baserpc.ClusterMemberListener;
import bifromq.re.baserpc.IClusterManager;
import com.google.common.collect.Sets;
import io.reactivex.rxjava3.core.Observable;
import io.reactivex.rxjava3.subjects.BehaviorSubject;

import java.util.Set;
import java.util.UUID;

public class TrafficDiscoveryImpl implements ITrafficDiscovery {
    private final BehaviorSubject<Set<Server>> serverListSubject = BehaviorSubject.create();
    private final UUID listerId;
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
        this.listerId = clusterManager.addClusterMemberListener(listener);
    }

    public Observable<Set<Server>> start() {
        return this.serverListSubject;
    }

    public void stop() {
        this.clusterManager.removeClusterMemberListener(this.listerId);
        this.serverListSubject.onComplete();
    }
}
