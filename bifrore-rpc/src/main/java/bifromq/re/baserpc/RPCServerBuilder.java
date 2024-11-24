package bifromq.re.baserpc;

import com.google.common.base.Preconditions;
import com.google.common.base.Strings;
import io.grpc.ServerServiceDefinition;
import io.netty.channel.EventLoopGroup;
import io.netty.handler.ssl.SslContext;
import lombok.extern.slf4j.Slf4j;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.Executor;


@Slf4j
public final class RPCServerBuilder {
    String id = UUID.randomUUID().toString() + "/" + hashCode();
    String host;
    int port = 0;
    EventLoopGroup bossEventLoopGroup;
    EventLoopGroup workerEventLoopGroup;
    SslContext sslContext;
    Executor executor;
    IClusterManager clusterManager;
    List<ServerServiceDefinition> serviceDefinitions = new ArrayList<>();

    public RPCServerBuilder id(String id) {
        if (!Strings.isNullOrEmpty(id)) {
            this.id = id;
        }
        return this;
    }

    public RPCServerBuilder host(String host) {
        this.host = host;
        return this;
    }

    public RPCServerBuilder port(int port) {
        Preconditions.checkArgument(port >= 0, "Port number must be non-negative");
        this.port = port;
        return this;
    }

    public RPCServerBuilder bossEventLoopGroup(EventLoopGroup bossEventLoopGroup) {
        this.bossEventLoopGroup = bossEventLoopGroup;
        return this;
    }

    public RPCServerBuilder workerEventLoopGroup(EventLoopGroup workerEventLoopGroup) {
        this.workerEventLoopGroup = workerEventLoopGroup;
        return this;
    }

    public RPCServerBuilder sslContext(SslContext sslContext) {
        if (sslContext != null) {
            Preconditions.checkArgument(sslContext.isServer(), "Server auth must be enabled");
        }
        this.sslContext = sslContext;
        return this;
    }

    public RPCServerBuilder clusterManager(IClusterManager clusterManager) {
        this.clusterManager = clusterManager;
        return this;
    }

    public RPCServerBuilder addService(ServerServiceDefinition serverServiceDefinition) {
        serviceDefinitions.add(serverServiceDefinition);
        return this;
    }

    public RPCServerBuilder executor(Executor executor) {
        this.executor = executor;
        return this;
    }

    public RPCServer build() {
        Preconditions.checkNotNull(id, "ID must be set");
        Preconditions.checkArgument(!serviceDefinitions.isEmpty());
        return new RPCServer(this);
    }
}