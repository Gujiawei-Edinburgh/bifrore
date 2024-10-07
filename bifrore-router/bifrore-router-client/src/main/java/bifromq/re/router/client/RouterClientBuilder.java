package bifromq.re.router.client;

import bifromq.re.baserpc.IRPCClient;
import bifromq.re.router.rpc.proto.RouterServiceGrpc;
import io.netty.channel.EventLoopGroup;
import io.netty.handler.ssl.SslContext;

import java.util.concurrent.Executor;

public class RouterClientBuilder {
    private Executor executor;
    private EventLoopGroup eventLoopGroup;
    private SslContext sslContext;

    public RouterClientBuilder executor(Executor executor) {
        this.executor = executor;
        return this;
    }

    public RouterClientBuilder eventLoopGroup(EventLoopGroup eventLoopGroup) {
        this.eventLoopGroup = eventLoopGroup;
        return this;
    }

    public RouterClientBuilder sslContext(SslContext sslContext) {
        this.sslContext = sslContext;
        return this;
    }

    public IRouterClient build() {
        return new RouterClient(IRPCClient.newBuilder()
                .serviceUniqueName(RouterServiceGrpc.getServiceDescriptor().getName())
                .executor(executor)
                .eventLoopGroup(eventLoopGroup)
                .sslContext(sslContext)
                .build());
    }
}
