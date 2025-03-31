package bifrore.processor.client;

import bifrore.baserpc.IRPCClient;
import bifrore.processor.rpc.proto.*;
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

    @Override
    public CompletableFuture<AddDestinationResponse> addDestination(AddDestinationRequest request) {
        return rpcClient.invoke(request, Collections.emptyMap(), ProcessorServiceGrpc.getAddDestinationMethod());
    }
}
