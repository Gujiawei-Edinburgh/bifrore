package bifrore.destination.plugin;

import bifrore.commontype.Message;
import lombok.extern.slf4j.Slf4j;
import org.pf4j.PluginManager;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.stream.Collectors;

@Slf4j
public class ProducerManager {
    private final Map<String, IProducer> producers;

    public ProducerManager(PluginManager pluginMgr) {
        producers = pluginMgr.getExtensions(IProducer.class).stream()
                        .collect(Collectors.toMap(IProducer::getName, e -> e));
        if (producers.isEmpty()) {
            log.warn("No producers registered, use DevOnly instead");
            IProducer devOnlyProducer = new DevOnlyProducer();
            producers.putIfAbsent(devOnlyProducer.getName(), new DevOnlyProducer());
        }
    }

    public CompletableFuture<Void> produce(List<String> destinations, Message message) {
        List<CompletableFuture<Boolean>> futures = new ArrayList<>();
        destinations.forEach(destination -> {
            IProducer producer = producers.get(destination);
            if (producer != null) {
                futures.add(producer.produce(message));
            }else {
                log.warn("No producer registered for {}", destination);
            }
        });
        return CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]))
                .whenComplete((v,e) -> {
                    if (e != null) {
                        log.error("Failed to produce message", e);
                    }
                });
    }

    public void close() {
        producers.values().forEach(IProducer::close);
        log.info("Producers closed");
    }
}
