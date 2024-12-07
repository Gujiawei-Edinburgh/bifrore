package bifrore.processor.client;

import bifrore.baserpc.RPCClientBuilder;
import bifrore.processor.rpc.proto.ProcessorServiceGrpc;

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
