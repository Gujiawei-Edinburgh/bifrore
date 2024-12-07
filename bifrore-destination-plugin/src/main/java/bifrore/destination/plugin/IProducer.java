package bifrore.destination.plugin;

import bifrore.commontype.Message;
import org.pf4j.ExtensionPoint;

import java.util.concurrent.CompletableFuture;

public interface IProducer extends ExtensionPoint {

    CompletableFuture<Boolean> produce(Message message);

    String getName();

    default void close() {}
}
