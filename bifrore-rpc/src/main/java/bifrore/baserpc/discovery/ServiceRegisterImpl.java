package bifrore.baserpc.discovery;

import bifrore.baserpc.IClusterManager;

import java.net.InetSocketAddress;

public class ServiceRegisterImpl implements IServiceRegister{
    private final IClusterManager clusterManager;

    public ServiceRegisterImpl(IClusterManager clusterManager) {
        this.clusterManager = clusterManager;
    }
    @Override
    public void register(String id, InetSocketAddress hostAddr) {
        clusterManager.addClusterMember(id + TrafficHelper.ServerInfoSeparator +
                hostAddr.getAddress().getHostAddress() +
                TrafficHelper.ServerInfoSeparator + hostAddr.getPort());
    }

    @Override
    public void unregister(String id, InetSocketAddress hostAddr) {
        clusterManager.removeClusterMember(id + TrafficHelper.ServerInfoSeparator +
                hostAddr.getAddress().getHostAddress() +
                TrafficHelper.ServerInfoSeparator + hostAddr.getPort());
    }
}
