package bifromq.re.processor.client;

import bifromq.re.baserpc.IRPCClient;
import bifromq.re.processor.rpc.proto.ProcessorServiceGrpc;
import io.netty.channel.EventLoopGroup;
import io.netty.handler.ssl.SslContext;

import java.util.concurrent.Executor;

public class ProcessorClientBuilder {
    private Executor executor;
    private EventLoopGroup eventLoopGroup;
    private SslContext sslContext;

    public ProcessorClientBuilder executor(Executor executor) {
        this.executor = executor;
        return this;
    }

    public ProcessorClientBuilder eventLoopGroup(EventLoopGroup eventLoopGroup) {
        this.eventLoopGroup = eventLoopGroup;
        return this;
    }

    public ProcessorClientBuilder sslContext(SslContext sslContext) {
        this.sslContext = sslContext;
        return this;
    }

    public IProcessorClient build() {
        return new ProcessorClient(IRPCClient.newBuilder()
                .serviceUniqueName(ProcessorServiceGrpc.getSubscribeMethod().getServiceName())
                .executor(executor)
                .eventLoopGroup(eventLoopGroup)
                .sslContext(sslContext)
                .build());
    }
}
