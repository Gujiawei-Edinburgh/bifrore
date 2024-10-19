package bifromq.re.baserpc.discovery;

import java.net.InetSocketAddress;

public interface IServiceRegister {
    void register(String id, InetSocketAddress hostAddr);
    void unregister(String id, InetSocketAddress hostAddr);
}
