package bifrore.baserpc;

import bifrore.baserpc.interceptor.ClientInterceptor;
import bifrore.baserpc.nameresolver.ServiceNameResolverProvider;
import bifrore.baserpc.util.NettyUtil;
import com.google.common.base.Preconditions;
import io.grpc.NameResolverRegistry;
import io.grpc.netty.NegotiationType;
import io.grpc.netty.NettyChannelBuilder;
import io.netty.channel.EventLoopGroup;
import io.netty.handler.ssl.SslContext;

import java.util.concurrent.*;

public final class RPCClientBuilder {
    private String serviceUniqueName;
    private Executor executor;
    private EventLoopGroup eventLoopGroup;
    private long keepAliveInSec;
    private long idleTimeoutInSec;
    private SslContext sslContext;
    private IClusterManager clusterManager;

    RPCClientBuilder() {
    }

    public RPCClientBuilder serviceUniqueName(String serviceUniqueName) {
        this.serviceUniqueName = serviceUniqueName;
        return this;
    }

    public RPCClientBuilder executor(Executor executor) {
        this.executor = executor;
        return this;
    }

    public RPCClientBuilder eventLoopGroup(EventLoopGroup eventLoopGroup) {
        this.eventLoopGroup = eventLoopGroup;
        return this;
    }

    public RPCClientBuilder keepAliveInSec(long keepAliveInSec) {
        this.keepAliveInSec = keepAliveInSec;
        return this;
    }

    public RPCClientBuilder idleTimeoutInSec(long idleTimeoutInSec) {
        this.idleTimeoutInSec = idleTimeoutInSec;
        return this;
    }

    public RPCClientBuilder sslContext(SslContext sslContext) {
        if (sslContext != null) {
            Preconditions.checkArgument(sslContext.isClient(), "Client auth must be enabled");
        }
        this.sslContext = sslContext;
        return this;
    }

    public RPCClientBuilder clusterManager(IClusterManager clusterManager) {
        this.clusterManager = clusterManager;
        return this;
    }

    public IRPCClient build() {
        Preconditions.checkNotNull(serviceUniqueName, "ServiceUniqueName must be set");
        NameResolverRegistry.getDefaultRegistry().register(ServiceNameResolverProvider.INSTANCE);
        ServiceNameResolverProvider.register(serviceUniqueName, clusterManager);
        NettyChannelBuilder channelBuilder = NettyChannelBuilder
                .forTarget(ServiceNameResolverProvider.SCHEME + "://" + serviceUniqueName)
                .defaultLoadBalancingPolicy("round_robin")
                .keepAliveTime(keepAliveInSec <= 0 ? 600 : keepAliveInSec, TimeUnit.SECONDS)
                .idleTimeout(idleTimeoutInSec <= 0 ? (365 * 24 * 3600) : idleTimeoutInSec, TimeUnit.SECONDS)
                .intercept(new ClientInterceptor())
                .executor(executor);
        if (sslContext != null) {
            channelBuilder.negotiationType(NegotiationType.TLS)
                    .sslContext(sslContext)
                    .overrideAuthority(serviceUniqueName);
        }else {
            channelBuilder.negotiationType(NegotiationType.PLAINTEXT);
        }
        if (eventLoopGroup != null) {
            channelBuilder.eventLoopGroup(eventLoopGroup)
                    .channelType(NettyUtil.determineSocketChannelClass(eventLoopGroup));
        }
        return new RPCClient(channelBuilder.build());
    }
}

