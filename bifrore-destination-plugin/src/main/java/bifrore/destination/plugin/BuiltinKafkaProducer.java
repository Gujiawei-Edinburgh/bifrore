package bifrore.destination.plugin;

import bifrore.commontype.Message;
import org.apache.kafka.clients.producer.KafkaProducer;
import org.apache.kafka.clients.producer.Producer;
import org.apache.kafka.clients.producer.ProducerConfig;
import org.apache.kafka.common.serialization.ByteArraySerializer;

import java.util.HashMap;
import java.util.Map;
import java.util.Properties;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;

public class BuiltinKafkaProducer implements IProducer{
    private final Map<String, Producer<byte[], byte[]>> callers = new HashMap<>();
    @Override
    public CompletableFuture<Boolean> produce(Message message, String callerId) {
        return null;
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
    public CompletableFuture<Boolean> closeCaller(String callerId) {
        Producer<byte[], byte[]> producer = callers.remove(callerId);
        producer.close();
        return CompletableFuture.completedFuture(true);
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
