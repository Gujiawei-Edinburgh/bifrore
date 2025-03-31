package bifrore.destination.plugin;

import bifrore.commontype.Message;
import lombok.extern.slf4j.Slf4j;

import java.util.HashMap;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;

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
    public CompletableFuture<Boolean> produce(Message message, String callerId) {
        DevOnlyCaller caller = callers.get(callerId);
        if (caller == null) {
            log.warn("Caller not found: {}", callerId);
            return CompletableFuture.completedFuture(false);
        }
        return caller.produce(message);
    }

    @Override
    public CompletableFuture<String> initCaller(Map<String, String> callerCfgMap) {
        DevOnlyCaller caller = new DevOnlyCaller();
        String callerId = this.getName() + IProducer.delimiter + UUID.randomUUID();
        callers.putIfAbsent(callerId, caller);
        return CompletableFuture.completedFuture(callerId);
    }

    @Override
    public CompletableFuture<Boolean> closeCaller(String callerId) {
        DevOnlyCaller caller = callers.remove(callerId);
        return CompletableFuture.completedFuture(caller != null);
    }

    @Override
    public String getName() {
        return "DevOnly";
    }
}
