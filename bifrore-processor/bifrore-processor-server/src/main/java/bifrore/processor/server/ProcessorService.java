package bifrore.processor.server;


import bifrore.processor.rpc.proto.*;
import bifrore.processor.worker.IProcessorWorker;
import io.grpc.stub.StreamObserver;
import lombok.extern.slf4j.Slf4j;

import java.util.concurrent.CompletableFuture;

import static bifrore.baserpc.UnaryResponse.response;

@Slf4j
public class ProcessorService extends ProcessorServiceGrpc.ProcessorServiceImplBase {

    private final IProcessorWorker processorWorker;

    public ProcessorService(IProcessorWorker processorWorker) {
        this.processorWorker = processorWorker;
    }

    @Override
    public void subscribe(SubscribeRequest request, StreamObserver<SubscribeResponse> responseObserver) {
        response(metadata -> {
            CompletableFuture<SubscribeResponse> future = new CompletableFuture<>();
            processorWorker.sub(request.getTopicFilter())
                    .whenComplete((v, e) -> {
                        if (e != null) {
                            log.error("Failed to subscribe topicFilter: {}", request.getTopicFilter(), e);
                            future.complete(SubscribeResponse.newBuilder()
                                    .setReqId(request.getReqId())
                                    .setCode(SubscribeResponse.Code.ERROR)
                                    .setReason(e.getMessage())
                                    .build());
                        }else {
                            future.complete(SubscribeResponse.newBuilder()
                                    .setReqId(request.getReqId())
                                    .setCode(SubscribeResponse.Code.OK)
                                    .build());
                        }
                    });
            return future;
        }, responseObserver);
    }

    @Override
    public void unsubscribe(UnsubscribeRequest request, StreamObserver<UnsubscribeResponse> responseObserver) {
        response(metadata -> {
            CompletableFuture<UnsubscribeResponse> future = new CompletableFuture<>();
            processorWorker.unsub(request.getTopicFilter())
                    .whenComplete((v, e) -> {
                        if (e != null) {
                            log.error("Failed to unsubscribe topicFilter: {}", request.getTopicFilter(), e);
                            future.complete(UnsubscribeResponse.newBuilder()
                                    .setReqId(request.getReqId())
                                    .setCode(UnsubscribeResponse.Code.ERROR)
                                    .setReason(e.getMessage())
                                    .build());
                        }else {
                            future.complete(UnsubscribeResponse.newBuilder()
                                    .setReqId(request.getReqId())
                                    .setCode(UnsubscribeResponse.Code.OK)
                                    .build());
                        }
                    });
            return future;
        }, responseObserver);
    }

    @Override
    public void addDestination(AddDestinationRequest request, StreamObserver<AddDestinationResponse> responseObserver) {
        response(metadata -> {
            CompletableFuture<AddDestinationResponse> future = new CompletableFuture<>();
            AddDestinationResponse.Builder builder = AddDestinationResponse.newBuilder();
            builder.setReqId(request.getReqId());
            processorWorker.addDestination(request.getDestinationType(), request.getDestinationCfgMap())
                    .whenComplete((v, e) -> {
                        if (e != null) {
                            log.error("Failed to add destination: {}", request.getDestinationCfgMap(), e);
                            builder.setCode(AddDestinationResponse.Code.ERROR).setReason(e.getMessage());
                            future.complete(builder.build());
                        }else {
                            builder.setCode(AddDestinationResponse.Code.OK);
                            builder.setDestinationId(v);
                            future.complete(builder.build());
                        }
                    });
            return future;
        }, responseObserver);
    }
}
