package bifromq.re.processor.server;


import bifromq.re.processor.rpc.proto.*;
import io.grpc.stub.StreamObserver;

import java.util.concurrent.CompletableFuture;

import static bifromq.re.baserpc.UnaryResponse.response;

public class ProcessorService extends ProcessorServiceGrpc.ProcessorServiceImplBase {
    @Override
    public void subscribe(SubscribeRequest request, StreamObserver<SubscribeResponse> responseObserver) {
        response(metadata -> {
            CompletableFuture<SubscribeResponse> future = new CompletableFuture<>();
            return future;
        }, responseObserver);
    }

    @Override
    public void unsubscribe(UnsubscribeRequest request, StreamObserver<UnsubscribeResponse> responseObserver) {
        response(metadata -> {
            CompletableFuture<UnsubscribeResponse> future = new CompletableFuture<>();
            return future;
        }, responseObserver);
    }
}
