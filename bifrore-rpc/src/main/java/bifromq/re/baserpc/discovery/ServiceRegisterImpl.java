package bifromq.re.baserpc.discovery;

import java.net.InetSocketAddress;
import java.util.Set;

public class ServiceRegisterImpl implements IServiceRegister{
    @Override
    public void register(String serviceUniqueName, String id, InetSocketAddress hostAddr) {
        Set<String> serviceSet = TrafficProvider.getInstance().getSet(serviceUniqueName);
        serviceSet.add(id + TrafficProvider.ServerInfoSeparator + hostAddr.getAddress().getHostAddress() +
                TrafficProvider.ServerInfoSeparator + hostAddr.getPort());
    }

    @Override
    public void unregister(String serviceUniqueName, String id, InetSocketAddress hostAddr) {
        Set<String> serviceSet = TrafficProvider.getInstance().getSet(serviceUniqueName);
        serviceSet.remove(id + TrafficProvider.ServerInfoSeparator + hostAddr.getAddress().getHostAddress() +
                TrafficProvider.ServerInfoSeparator + hostAddr.getPort());
    }
}
