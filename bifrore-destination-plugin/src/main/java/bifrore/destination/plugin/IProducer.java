package bifrore.destination.plugin;

import bifrore.commontype.Message;
import org.pf4j.ExtensionPoint;

import java.util.Map;
import java.util.concurrent.CompletableFuture;

public interface IProducer extends ExtensionPoint {

    String delimiter = "/";

    CompletableFuture<Boolean> produce(Message message, String callerId);

    CompletableFuture<String> initCaller(Map<String, String> callerCfgMap);

    CompletableFuture<Boolean> closeCaller(String callerId);

    String getName();

    default void close() {}
}
