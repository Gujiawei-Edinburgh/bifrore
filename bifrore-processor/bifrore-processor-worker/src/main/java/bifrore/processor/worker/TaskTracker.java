package bifrore.processor.worker;

import com.hivemq.client.mqtt.mqtt5.message.publish.Mqtt5Publish;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;

public class TaskTracker {
    private Map<Mqtt5Publish, List<CompletableFuture<Void>>> futureMap = new HashMap<>();

    public void track(Mqtt5Publish publish) {
        List<CompletableFuture<Void>> futures = List.of(new CompletableFuture<>(), new CompletableFuture<>());
        futureMap.put(publish, futures);
    }

    public List<CompletableFuture<Void>> getFutures(Mqtt5Publish publish) {
        return futureMap.get(publish);
    }
}
