package bifromq.re.processor.worker;

import java.util.concurrent.CompletableFuture;

public interface IProcessorWorker {
    static ProcessorWorkerBuilder newBuilder() {
        return new ProcessorWorkerBuilder();
    }
    void start();

    CompletableFuture<Void> sub(String topic);

    CompletableFuture<Void> unsub(String topic);

    void close();
}
