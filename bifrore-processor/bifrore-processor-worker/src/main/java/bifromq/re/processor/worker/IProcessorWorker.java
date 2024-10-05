package bifromq.re.processor.worker;

import java.util.concurrent.CompletableFuture;

public interface IProcessorWorker {
    void start();

    CompletableFuture<Void> sub(String topic);

    CompletableFuture<Void> unsub(String topic);

    void close();
}
