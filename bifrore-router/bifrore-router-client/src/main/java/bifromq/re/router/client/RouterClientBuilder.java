package bifromq.re.router.client;

import bifromq.re.baserpc.RPCClientBuilder;
import bifromq.re.router.rpc.proto.RouterServiceGrpc;

public class RouterClientBuilder {
    private RPCClientBuilder rpcClientBuilder;

    public RouterClientBuilder rpcClientBuilder(RPCClientBuilder rpcClientBuilder) {
        this.rpcClientBuilder = rpcClientBuilder;
        return this;
    }

    public IRouterClient build() {
        return new RouterClient(rpcClientBuilder
                .serviceUniqueName(RouterServiceGrpc.getServiceDescriptor().getName())
                .build());
    }
}
