package bifromq.re.baserpc.discovery;

import com.hazelcast.core.Hazelcast;
import com.hazelcast.core.HazelcastInstance;

public class TrafficProvider {
    public static String ServerInfoSeparator = ",";
    private static HazelcastInstance hazelcastInstance;

    public static HazelcastInstance getInstance() {
        if (hazelcastInstance == null) {
            hazelcastInstance = Hazelcast.newHazelcastInstance();
            return hazelcastInstance;
        }
        return hazelcastInstance;
    }
}
