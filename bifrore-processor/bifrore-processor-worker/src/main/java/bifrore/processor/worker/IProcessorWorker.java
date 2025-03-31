package bifrore.processor.worker;

import java.util.Map;
import java.util.concurrent.CompletableFuture;

public interface IProcessorWorker {
    static ProcessorWorkerBuilder newBuilder() {
        return new ProcessorWorkerBuilder();
    }
    void start();

    CompletableFuture<Void> sub(String topic);

    CompletableFuture<Void> unsub(String topic);

    CompletableFuture<String> addDestination(String destinationType, Map<String, String> destinationCfg);

    void close();
}
