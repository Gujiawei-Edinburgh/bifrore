package bifrore.processor.client;

import bifrore.processor.rpc.proto.AddDestinationRequest;
import bifrore.processor.rpc.proto.AddDestinationResponse;
import bifrore.processor.rpc.proto.SubscribeRequest;
import bifrore.processor.rpc.proto.SubscribeResponse;
import bifrore.processor.rpc.proto.UnsubscribeRequest;
import bifrore.processor.rpc.proto.UnsubscribeResponse;

import java.util.concurrent.CompletableFuture;

public interface IProcessorClient {
    static ProcessorClientBuilder newBuilder() {
        return new ProcessorClientBuilder();
    }

    CompletableFuture<SubscribeResponse> subscribe(SubscribeRequest request);

    CompletableFuture<UnsubscribeResponse> unsubscribe(UnsubscribeRequest request);

    CompletableFuture<AddDestinationResponse> addDestination(AddDestinationRequest request);
}
