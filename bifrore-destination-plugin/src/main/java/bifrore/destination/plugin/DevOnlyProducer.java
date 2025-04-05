package bifrore.destination.plugin;

import bifrore.commontype.Message;
import bifrore.monitoring.metrics.SysMeter;
import lombok.extern.slf4j.Slf4j;

import java.util.HashMap;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;

import static bifrore.monitoring.metrics.SysMetric.DestinationMissCount;

@Slf4j
public class DevOnlyProducer implements IProducer {
    private final Map<String, DevOnlyCaller> callers = new HashMap<>();

    static class DevOnlyCaller {
        CompletableFuture<Boolean> produce(Message message) {
            log.info("producing message: {}", message);
            return CompletableFuture.completedFuture(true);
        }
    }

    @Override
    public CompletableFuture<Void> produce(Message message, String callerId) {
        DevOnlyCaller caller = callers.get(callerId);
        if (caller == null) {
            SysMeter.INSTANCE.recordCount(DestinationMissCount);
            log.warn("Caller not found: {}", callerId);
        }
        return CompletableFuture.completedFuture(null);
    }

    @Override
    public CompletableFuture<String> initCaller(Map<String, String> callerCfgMap) {
        String callerId = createProducerInstance(Optional.empty());
        return CompletableFuture.completedFuture(callerId);
    }

    @Override
    public CompletableFuture<Void> syncCaller(String callerId, Map<String, String> callerCfgMap) {
        if (!callers.containsKey(callerId)) {
            createProducerInstance(Optional.of(callerId));
        }
        return CompletableFuture.completedFuture(null);
    }

    @Override
    public CompletableFuture<Void> closeCaller(String callerId) {
        CompletableFuture<Void> future = new CompletableFuture<>();
        DevOnlyCaller caller = callers.remove(callerId);
        if (caller == null) {
            future.completeExceptionally(new IllegalStateException("Caller not found: " + callerId));
        }else {
            future.complete(null);
        }
        return future;
    }

    @Override
    public String getName() {
        return "DevOnly";
    }

    private String createProducerInstance(Optional<String> callerIdPresent) {
        DevOnlyCaller caller = new DevOnlyCaller();
        String callerId = callerIdPresent.orElseGet(() -> this.getName() + IProducer.DELIMITER + UUID.randomUUID());
        callers.putIfAbsent(callerId, caller);
        return callerId;
    }
}
