package bifrore.destination.plugin;

import bifrore.commontype.Message;
import lombok.extern.slf4j.Slf4j;

import java.util.concurrent.CompletableFuture;

@Slf4j
public class DevOnlyProducer implements IProducer {

    @Override
    public CompletableFuture<Boolean> produce(Message message) {
        log.info("producing message: {}", message);
        return CompletableFuture.completedFuture(true);
    }

    @Override
    public String getName() {
        return "DevOnly";
    }
}
