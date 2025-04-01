package bifrore.destination.plugin;

import bifrore.commontype.Message;
import com.hazelcast.map.IMap;
import lombok.extern.slf4j.Slf4j;
import org.pf4j.PluginManager;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.stream.Collectors;

import static bifrore.common.type.MapMessageUtil.deserialize;
import static bifrore.common.type.MapMessageUtil.serialize;

@Slf4j
public class ProducerManager {
    private final Map<String, IProducer> destinations;
    private final IMap<String, byte[]> callerCfgs;

    public ProducerManager(PluginManager pluginMgr, IMap<String, byte[]> callerCfgs) {
        this.destinations = pluginMgr.getExtensions(IProducer.class).stream()
                        .collect(Collectors.toMap(IProducer::getName, e -> e));
        this.callerCfgs = callerCfgs;
        this.restoreCallers().whenComplete((v, e) -> {
            if (e != null) {
                log.error("Failed to restore callers", e);
            }
        });
        if (destinations.isEmpty()) {
            log.info("No producers registered, use DevOnly and Kafka instead");
            IProducer devOnlyProducer = new DevOnlyProducer();
            IProducer builtinKafkaProducer = new BuiltinKafkaProducer();
            destinations.putIfAbsent(devOnlyProducer.getName(), new DevOnlyProducer());
            destinations.putIfAbsent(builtinKafkaProducer.getName(), new BuiltinKafkaProducer());
        }
    }

    public CompletableFuture<Void> produce(List<String> destinations, Message message) {
        List<CompletableFuture<Boolean>> futures = new ArrayList<>();
        destinations.forEach(destination -> {
            String[] info = destination.split(IProducer.delimiter);
            IProducer producer = this.destinations.get(info[0]);
            if (producer != null) {
                futures.add(producer.produce(message, destination));
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

    public CompletableFuture<String> createDestinationCaller(String destinationName, Map<String, String> callerCfg) {
        CompletableFuture<String> future = new CompletableFuture<>();
        IProducer producer = destinations.get(destinationName);
        if (producer == null) {
            future.completeExceptionally(new IllegalArgumentException("No producer registered for " + destinationName));
            return future;
        }
        producer.initCaller(callerCfg).whenComplete((v, e) -> {
            if (e != null) {
                future.completeExceptionally(e);
            }else {
                callerCfgs.put(v, serialize(callerCfg));
                future.complete(v);
            }
        });
        return future;
    }

    private CompletableFuture<Void> restoreCallers() {
        List<CompletableFuture<String>> futures = new ArrayList<>();
        callerCfgs.forEach((destinationId, cfg) -> {
            String producerName = destinationId.split(IProducer.delimiter)[0];
            IProducer producer = destinations.get(producerName);
            try {
                CompletableFuture<String> future = producer.initCaller(deserialize(cfg));
                futures.add(future);
            }catch (Exception ex) {
                log.error("Failed to create caller for {}", producerName, ex);
                CompletableFuture<String> future = new CompletableFuture<>();
                future.completeExceptionally(ex);
                futures.add(future);
            }
        });
        return CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]));
    }

    public void close() {
        destinations.values().forEach(IProducer::close);
        log.info("Producers closed");
    }
}
