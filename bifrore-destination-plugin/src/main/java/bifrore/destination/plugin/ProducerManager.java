package bifrore.destination.plugin;

import bifrore.commontype.MapMessage;
import bifrore.commontype.Message;
import bifrore.monitoring.metrics.SysMeter;
import com.google.protobuf.InvalidProtocolBufferException;
import com.hazelcast.core.EntryEvent;
import com.hazelcast.map.IMap;
import com.hazelcast.map.listener.EntryAddedListener;
import com.hazelcast.map.listener.EntryRemovedListener;
import lombok.extern.slf4j.Slf4j;
import org.pf4j.PluginManager;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.stream.Collectors;

import static bifrore.common.type.SerializationUtil.deserializeMap;
import static bifrore.common.type.SerializationUtil.serializeMap;
import static bifrore.monitoring.metrics.SysMetric.DestinationNumGauge;
import static bifrore.monitoring.metrics.SysMetric.ProducerInboundCount;
import static bifrore.monitoring.metrics.SysMetric.ProducerMissCount;

@Slf4j
public class ProducerManager {
    private final Map<String, IProducer> destinations;
    private final IMap<String, byte[]> callerCfgs;
    private final SysMeter meter = SysMeter.INSTANCE;

    static class EntryListenerImpl implements EntryAddedListener<String, byte[]>, EntryRemovedListener<String, byte[]> {
        ProducerManager manager;
        EntryListenerImpl(ProducerManager manager) {
            this.manager = manager;
        }
        @Override
        public void entryAdded(EntryEvent<String, byte[]> event) {
            try {
                this.manager.syncDestinationCreation(event.getKey(),
                        MapMessage.parseFrom(event.getValue()).getMapMessageMap());
            } catch (InvalidProtocolBufferException e) {
                log.error("Fail to add the entry: {}", event.getKey(), e);
            }
        }

        @Override
        public void entryRemoved(EntryEvent<String, byte[]> event) {
            this.manager.deleteDestinationCaller(event.getKey());
        }
    }

    public ProducerManager(PluginManager pluginMgr, IMap<String, byte[]> callerCfgs) {
        this.destinations = pluginMgr.getExtensions(IProducer.class).stream()
                        .collect(Collectors.toMap(IProducer::getName, e -> e));
        this.callerCfgs = callerCfgs;
        this.callerCfgs.addEntryListener(new EntryListenerImpl(this), true);
        if (destinations.isEmpty()) {
            log.info("No producers registered, use DevOnly and Kafka instead");
            IProducer devOnlyProducer = new DevOnlyProducer();
            IProducer builtinKafkaProducer = new BuiltinKafkaProducer();
            destinations.putIfAbsent(devOnlyProducer.getName(), new DevOnlyProducer());
            destinations.putIfAbsent(builtinKafkaProducer.getName(), new BuiltinKafkaProducer());
        }
    }

    public void start() {
        this.restoreCallers().whenComplete((v, e) -> {
            if (e != null) {
                log.error("Failed to restore callers", e);
            }
        });
        meter.startGauge(DestinationNumGauge, callerCfgs::size);
    }

    public void stop() {
        destinations.values().forEach(IProducer::close);
        meter.stopGauge(DestinationNumGauge);
        log.info("Producers closed");
    }

    public CompletableFuture<Void> produce(List<String> destinations, Message message) {
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        destinations.forEach(destination -> {
            String[] info = destination.split(IProducer.DELIMITER);
            IProducer producer = this.destinations.get(info[0]);
            if (producer != null) {
                meter.recordCount(ProducerInboundCount);
                futures.add(producer.produce(message, destination));
            }else {
                meter.recordCount(ProducerMissCount);
                log.warn("No producer registered for {}, ignore it silently", destination);
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
                callerCfgs.put(v, serializeMap(callerCfg));
                future.complete(v);
            }
        });
        return future;
    }

    public CompletableFuture<Void> deleteDestinationCaller(String destinationId) {
        CompletableFuture<Void> future = new CompletableFuture<>();
        String destinationName = destinationId.split(IProducer.DELIMITER)[0];
        IProducer producer = destinations.get(destinationName);
        if (producer == null) {
            future.completeExceptionally(new IllegalArgumentException("No producer registered for " + destinationName));
            return future;
        }
        producer.closeCaller(destinationId).whenComplete((v, e) -> {
           if (e != null) {
               log.error("Failed to close caller: {}", destinationId, e);
               future.completeExceptionally(e);
           }else {
               callerCfgs.remove(destinationId);
               future.complete(v);
           }
        });
        return future;
    }

    public Map<String, MapMessage> listAllDestinations() {
        Map<String, MapMessage> snapshot = new HashMap<>();
        callerCfgs.forEach((k,v) -> {
            try {
                MapMessage message = MapMessage.parseFrom(v);
                snapshot.put(k, message);
            } catch (InvalidProtocolBufferException e) {
                log.error("Fail to parse the map message: {}", v, e);
            }
        });
        return snapshot;
    }

    private void syncDestinationCreation(String callerId, Map<String, String> callerCfg) {
        String destinationName = callerId.split(IProducer.DELIMITER)[0];
        IProducer producer = destinations.get(destinationName);
        if (producer == null) {
            log.error("No producer registered for {}", callerId);
            return;
        }
        producer.syncCaller(callerId, callerCfg).whenComplete((v, e) -> {
            if (e != null) {
                log.error("Failed to sync caller: {}", callerId, e);
            }
        });
    }

    private CompletableFuture<Void> restoreCallers() {
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        callerCfgs.forEach((destinationId, cfg) -> {
            String producerName = destinationId.split(IProducer.DELIMITER)[0];
            IProducer producer = destinations.get(producerName);
            try {
                CompletableFuture<Void> future = producer.syncCaller(destinationId, deserializeMap(cfg));
                futures.add(future);
            }catch (Exception ex) {
                log.error("Failed to create caller for {}", producerName, ex);
                CompletableFuture<Void> future = new CompletableFuture<>();
                future.completeExceptionally(ex);
                futures.add(future);
            }
        });
        return CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]));
    }
}
