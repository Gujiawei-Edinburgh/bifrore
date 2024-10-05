package bifromq.re.baserpc.discovery;

import java.net.InetSocketAddress;

public interface IServiceRegister {
    void register(String serviceUniqueName, String id, InetSocketAddress hostAddr);
    void unregister(String serviceUniqueName, String id, InetSocketAddress hostAddr);
}
