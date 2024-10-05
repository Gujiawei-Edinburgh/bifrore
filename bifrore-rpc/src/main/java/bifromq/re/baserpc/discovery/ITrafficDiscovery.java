package bifromq.re.baserpc.discovery;

import io.reactivex.rxjava3.core.Observable;

import java.net.InetSocketAddress;
import java.net.SocketAddress;
import java.util.Set;

public interface ITrafficDiscovery {
    Observable<Set<Server>> start();

    void stop();

    static ITrafficDiscovery getInstance(String serverUniqueName) {
        return new TrafficDiscoveryImpl(serverUniqueName);
    }

    class Server {
        public final String id;
        public final SocketAddress hostAddr;

        public Server(String serverInfo) {
            String[] serverMetadata = serverInfo.split(TrafficProvider.ServerInfoSeparator);
            this.id = serverMetadata[0];
            this.hostAddr = new InetSocketAddress(serverMetadata[1], Integer.parseInt(serverMetadata[2]));
        }
    }
}
