package bifrore.destination.plugin;

import bifrore.commontype.Message;
import io.micrometer.core.instrument.Metrics;
import io.micrometer.core.instrument.binder.jvm.ExecutorServiceMetrics;
import lombok.extern.slf4j.Slf4j;
import org.apache.kafka.clients.producer.KafkaProducer;
import org.apache.kafka.clients.producer.Producer;
import org.apache.kafka.clients.producer.ProducerConfig;
import org.apache.kafka.clients.producer.ProducerRecord;
import org.apache.kafka.common.serialization.ByteArraySerializer;

import java.util.HashMap;
import java.util.Map;
import java.util.Properties;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import java.util.concurrent.Executors;

@Slf4j
public class BuiltinKafkaProducer implements IProducer{
    private final Map<String, Producer<byte[], byte[]>> callers = new HashMap<>();
    private final Executor ioExecutor;

    public BuiltinKafkaProducer() {
        this.ioExecutor = ExecutorServiceMetrics.monitor(Metrics.globalRegistry,
                Executors.newFixedThreadPool(1), "bifrore-kafka-flush");
    }

    @Override
    public CompletableFuture<Void> produce(Message message, String callerId) {
        CompletableFuture<Void> future = new CompletableFuture<>();
        Producer<byte[], byte[]> producer = callers.get(callerId);
        if (producer == null) {
            log.warn("No producer found for callerId={}, silently return", callerId);
            future.complete(null);
            return future;
        }
        ProducerRecord<byte[], byte[]> record = new ProducerRecord<>(
                message.getTopic(), null, message.getPayload().toByteArray());
        producer.send(record, (metadata, exception) -> {
            if (exception != null) {
                log.error("Failed to send message", exception);
                future.completeExceptionally(exception);
            }else {
                future.complete(null);
            }
        });
        return future;
    }

    @Override
    public CompletableFuture<String> initCaller(Map<String, String> callerCfgMap) {
        Properties props = new Properties();
        props.putAll(callerCfgMap);
        props.putIfAbsent(ProducerConfig.KEY_SERIALIZER_CLASS_CONFIG, ByteArraySerializer.class.getName());
        props.putIfAbsent(ProducerConfig.VALUE_SERIALIZER_CLASS_CONFIG, ByteArraySerializer.class.getName());
        String callerId = this.getName() + IProducer.delimiter + UUID.randomUUID();
        callers.putIfAbsent(callerId, new KafkaProducer<>(props));
        return CompletableFuture.completedFuture(callerId);
    }

    @Override
    public CompletableFuture<Void> closeCaller(String callerId) {
        CompletableFuture<Void> future = new CompletableFuture<>();
        Producer<byte[], byte[]> producer = callers.remove(callerId);
        if (producer != null) {
            CompletableFuture.runAsync(producer::flush, this.ioExecutor)
                    .whenComplete((v, e) -> {
                        if (e != null) {
                            future.completeExceptionally(e);
                        }else {
                            future.complete(null);
                        }
                        producer.close();
                    });
        }else {
            future.completeExceptionally(new IllegalStateException("Caller not found: " + callerId));
        }
        return future;
    }

    @Override
    public String getName() {
        return "kafka";
    }

    @Override
    public void close() {
        callers.forEach((callerId, producer) -> producer.close());
    }
}
