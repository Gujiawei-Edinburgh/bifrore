package bifrore.baserpc;

import java.util.Set;

public interface ClusterMemberListener {
    void onMemberJoin(Set<String> members);
    void onMemberLeave(Set<String> members);
}
