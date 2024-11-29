package bifromq.re.processor.client;

import bifromq.re.processor.rpc.proto.SubscribeRequest;
import bifromq.re.processor.rpc.proto.SubscribeResponse;
import bifromq.re.processor.rpc.proto.UnsubscribeRequest;
import bifromq.re.processor.rpc.proto.UnsubscribeResponse;

import java.util.concurrent.CompletableFuture;

public interface IProcessorClient {
    static ProcessorClientBuilder newBuilder() {
        return new ProcessorClientBuilder();
    }

    CompletableFuture<SubscribeResponse> subscribe(SubscribeRequest request);

    CompletableFuture<UnsubscribeResponse> unsubscribe(UnsubscribeRequest request);
}
