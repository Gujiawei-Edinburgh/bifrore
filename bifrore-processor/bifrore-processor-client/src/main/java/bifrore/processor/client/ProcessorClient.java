package bifrore.processor.client;

import bifrore.baserpc.IRPCClient;

import bifrore.processor.rpc.proto.AddDestinationRequest;
import bifrore.processor.rpc.proto.AddDestinationResponse;
import bifrore.processor.rpc.proto.DeleteDestinationRequest;
import bifrore.processor.rpc.proto.DeleteDestinationResponse;
import bifrore.processor.rpc.proto.ListDestinationRequest;
import bifrore.processor.rpc.proto.ListDestinationResponse;
import bifrore.processor.rpc.proto.ProcessorServiceGrpc;
import bifrore.processor.rpc.proto.SubscribeRequest;
import bifrore.processor.rpc.proto.SubscribeResponse;
import bifrore.processor.rpc.proto.UnsubscribeRequest;
import bifrore.processor.rpc.proto.UnsubscribeResponse;
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

    @Override
    public CompletableFuture<DeleteDestinationResponse> deleteDestination(DeleteDestinationRequest request) {
        return rpcClient.invoke(request, Collections.emptyMap(), ProcessorServiceGrpc.getDeleteDestinationMethod());
    }

    @Override
    public CompletableFuture<ListDestinationResponse> listDestination(ListDestinationRequest request) {
        return rpcClient.invoke(request, Collections.emptyMap(), ProcessorServiceGrpc.getListDestinationsMethod());
    }
}
