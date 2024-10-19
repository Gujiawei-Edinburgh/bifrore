package bifromq.re.baserpc;

import java.util.UUID;

public interface IClusterManager {
    static ClusterManagerBuilder newBuilder() {
        return new ClusterManagerBuilder();
    }

    void addClusterMember(String clusterMember);
    void removeClusterMember(String clusterMember);
    UUID addClusterMemberListener(ClusterMemberListener listener);
    void removeClusterMemberListener(UUID listenerId);
    void close();
}
