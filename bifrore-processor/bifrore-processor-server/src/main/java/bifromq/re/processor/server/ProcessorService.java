package bifromq.re.processor.server;


import bifromq.re.processor.rpc.proto.*;
import bifromq.re.processor.worker.IProcessorWorker;
import io.grpc.stub.StreamObserver;
import lombok.extern.slf4j.Slf4j;

import java.util.concurrent.CompletableFuture;

import static bifromq.re.baserpc.UnaryResponse.response;

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
}
