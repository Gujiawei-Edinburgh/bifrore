package bifromq.re.baserpc;

import bifromq.re.baserpc.discovery.IServiceRegister;
import bifromq.re.baserpc.discovery.ServiceRegisterImpl;
import bifromq.re.baserpc.interceptor.ServerInterceptor;
import io.grpc.Server;
import io.grpc.ServerBuilder;
import io.grpc.ServerInterceptors;
import io.grpc.ServerServiceDefinition;
import io.grpc.netty.NettyServerBuilder;
import lombok.extern.slf4j.Slf4j;

import java.io.IOException;
import java.net.InetSocketAddress;
import java.util.List;

@Slf4j
class RPCServer implements IRPCServer {
    private final String id;
    private final List<ServerServiceDefinition> serviceDefinitions;
    private final Server server;
    private final IServiceRegister serviceRegister = new ServiceRegisterImpl();

    RPCServer(RPCServerBuilder builder) {
        NettyServerBuilder nettyServerBuilder = NettyServerBuilder
                .forAddress(new InetSocketAddress(builder.host, builder.port))
                .permitKeepAliveWithoutCalls(true)
                .maxInboundMessageSize(Integer.MAX_VALUE)
                .executor(builder.executor);
        this.serviceDefinitions=  builder.serviceDefinitions;
        bindServices(nettyServerBuilder);
        this.server = nettyServerBuilder.build();
        this.id = builder.id;
    }

    private void bindServices(ServerBuilder<?> builder) {
        serviceDefinitions.forEach(each -> builder.addService(ServerInterceptors.intercept(each,
                new ServerInterceptor())));
    }

    public String id() {
        return id;
    }

    public void start() {
        try {
            server.start();
            serviceDefinitions.forEach(each -> {
                String serviceName = each.getServiceDescriptor().getName();
                log.info("Start server register for service: {}", serviceName);
                serviceRegister.register(serviceName, id, (InetSocketAddress) server.getListenSockets().get(0));
            });
            log.info("RPCServer started");
        }catch (IOException e) {
            throw new IllegalStateException("Unable to start rpc server", e);
        }
    }

    public void shutdown() {
        serviceDefinitions.forEach(each -> {
            String serviceName = each.getServiceDescriptor().getName();
            log.debug("Unregistering server for service: {}", serviceName);
            serviceRegister.unregister(serviceName, id, (InetSocketAddress) server.getListenSockets().get(0));
        });
        shutdownInternalServer();
    }

    private void shutdownInternalServer() {
        try {
            server.shutdownNow();
            server.awaitTermination();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }
}
