package bifromq.re.processor.client;

import bifromq.re.baserpc.IRPCClient;
import bifromq.re.processor.rpc.proto.*;
import lombok.extern.slf4j.Slf4j;

import java.util.Collections;
import java.util.concurrent.CompletableFuture;

@Slf4j
final class ProcessorClient implements IProcessorClient {
    private final IRPCClient rpcClient;

    ProcessorClient(IRPCClient rpcClient) {
        super();
        this.rpcClient = rpcClient;
    }

    @Override
    public CompletableFuture<SubscribeResponse> subscribe(SubscribeRequest request) {
        return rpcClient.invoke(request, Collections.emptyMap(), ProcessorServiceGrpc.getSubscribeMethod());
    }

    @Override
    public CompletableFuture<UnsubscribeResponse> unsubscribe(UnsubscribeRequest request) {
        return  rpcClient.invoke(request, Collections.emptyMap(), ProcessorServiceGrpc.getUnsubscribeMethod());
    }
}
