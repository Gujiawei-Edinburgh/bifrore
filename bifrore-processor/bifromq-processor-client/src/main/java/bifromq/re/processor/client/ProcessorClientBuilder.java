package bifromq.re.processor.client;

import bifromq.re.baserpc.IRPCClient;
import bifromq.re.baserpc.RPCClientBuilder;
import bifromq.re.processor.rpc.proto.ProcessorServiceGrpc;
import io.netty.channel.EventLoopGroup;
import io.netty.handler.ssl.SslContext;

import java.util.concurrent.Executor;

public class ProcessorClientBuilder {
    private RPCClientBuilder rpcClientBuilder;

    public ProcessorClientBuilder rpcClientBuilder(RPCClientBuilder rpcClientBuilder) {
        this.rpcClientBuilder = rpcClientBuilder;
        return this;
    }

    public IProcessorClient build() {
        return new ProcessorClient(rpcClientBuilder
                .serviceUniqueName(ProcessorServiceGrpc.getSubscribeMethod().getServiceName())
                .build());
    }
}
