package bifrore.destination.plugin;

import bifrore.commontype.Message;
import org.pf4j.ExtensionPoint;

import java.util.Map;
import java.util.concurrent.CompletableFuture;

public interface IProducer extends ExtensionPoint {

    String DELIMITER = "/";

    CompletableFuture<Void> produce(Message message, String callerId);

    CompletableFuture<String> initCaller(Map<String, String> callerCfgMap);

    CompletableFuture<Void> syncCaller(String callerId, Map<String, String> callerCfgMap);

    CompletableFuture<Void> closeCaller(String callerId);

    String getName();

    default void close() {}
}
