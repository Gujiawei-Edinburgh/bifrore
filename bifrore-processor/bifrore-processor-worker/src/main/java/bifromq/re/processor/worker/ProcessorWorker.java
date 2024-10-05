package bifromq.re.processor.worker;

import bifromq.re.commontype.Message;
import com.hivemq.client.mqtt.mqtt5.Mqtt5AsyncClient;
import com.hivemq.client.mqtt.mqtt5.Mqtt5Client;
import com.hivemq.client.mqtt.mqtt5.message.auth.Mqtt5SimpleAuth;
import com.hivemq.client.mqtt.mqtt5.message.connect.Mqtt5Connect;
import com.hivemq.client.mqtt.mqtt5.message.connect.connack.Mqtt5ConnAckReasonCode;
import com.hivemq.client.mqtt.mqtt5.message.publish.Mqtt5Publish;
import com.hivemq.client.mqtt.mqtt5.message.subscribe.Mqtt5Subscribe;
import com.hivemq.client.mqtt.mqtt5.message.unsubscribe.Mqtt5Unsubscribe;
import io.reactivex.rxjava3.subjects.PublishSubject;
import lombok.extern.slf4j.Slf4j;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedList;
import java.util.List;
import java.util.concurrent.CompletableFuture;

@Slf4j
class ProcessorWorker implements IProcessorWorker {
    private int clientNum;
    private String userName;
    private String password;
    private boolean cleanStart;
    private boolean ordered;
    private long sessionExpiryInterval;
    private String host;
    private int port;
    private String clientPrefix;
    private IProducer delegator;
    private List<Mqtt5AsyncClient> clients = new LinkedList<>();
    private final PublishSubject<Message> emitter = PublishSubject.create();
    private final String DEFAULT_CLIENT_PREFIX = "bifromq-ruleEngine-";

    ProcessorWorker(ProcessorWorkerBuilder builder) {
//        this.emitter.doOnComplete(delegator::close)
//                .subscribe(delegator::send);
    }

    @Override
    public void start() {
        initClients();
    }

    @Override
    public CompletableFuture<Void> sub(String topic) {
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        clients.forEach(asyncClient -> {
            CompletableFuture<Void> future = new CompletableFuture<>();
            futures.add(future);
            Mqtt5Subscribe mqtt5Subscribe = Mqtt5Subscribe.builder()
                    .topicFilter(getTopicFilter(topic))
                    .noLocal(false)
                    .build();
            asyncClient.subscribe(mqtt5Subscribe, publish -> emitter.onNext(handlePublishedMsg(publish)))
                    .whenComplete(((mqtt5SubAck, error) -> {
                        if (error != null) {
                            log.error("Failed to subscribe: {}", mqtt5Subscribe);
                            future.completeExceptionally(new RuntimeException("Init client error"));
                        }
                        future.complete(null);
                    }));
        });
        return CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]))
                .whenComplete((v,e) -> {
                    if (e != null) {
                        log.error("Failed to init clients: ", e);
                    }else {
                        log.info("Subscribe clients ok, clientNum: {}", clientNum);
                    }
                });
    }

    @Override
    public CompletableFuture<Void> unsub(String topic) {
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        clients.forEach(asyncClient -> {
            CompletableFuture<Void> future = new CompletableFuture<>();
            futures.add(future);
            asyncClient.unsubscribe(Mqtt5Unsubscribe.builder().topicFilter(getTopicFilter(topic)).build())
                    .whenComplete(((mqtt5UnsubAck, error) -> {
                        if (error != null) {
                            log.error("Failed to unsubscribe topicFilter: {}", topic);
                        }
                    }));
        });
        return CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]))
                .whenComplete((v,e) -> {
                    if (e != null) {
                        log.error("Failed to unsubscribe clients: ", e);
                    }else {
                        log.info("Unsubscribe clients ok, clientNum: {}", clientNum);
                    }
                });
    }

    @Override
    public void close() {
        closeClients().thenAccept(v -> {
            clients.clear();
            emitter.onComplete();
        });
    }

    private void initClients() {
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        for (int idx = 0; idx < clientNum; idx++) {
            String identifier = clientPrefix + idx;
            Mqtt5AsyncClient asyncClient = Mqtt5Client.builder()
                    .identifier(identifier)
                    .serverHost(host)
                    .serverPort(port)
                    .buildAsync();
            CompletableFuture<Void> future = new CompletableFuture<>();
            futures.add(future);
            Mqtt5Connect mqtt5Connect = Mqtt5Connect.builder()
                    .simpleAuth(Mqtt5SimpleAuth.builder()
                            .username(userName)
                            .password(password.getBytes(StandardCharsets.UTF_8))
                            .build())
                    .cleanStart(cleanStart)
                    .sessionExpiryInterval(sessionExpiryInterval)
                    .build();
            asyncClient.connect(mqtt5Connect).thenAccept(mqtt5ConnAck -> {
                if (mqtt5ConnAck.getReasonCode() != Mqtt5ConnAckReasonCode.SUCCESS) {
                    log.error("ClientId: {} connect to broker failed: {}", identifier, mqtt5ConnAck.getReasonCode());
                }
            });
        }
        CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]))
                .whenComplete((v,e) -> {
                    if (e != null) {
                        log.error("Failed to init clients: ", e);
                    }else {
                        log.info("Init clients ok, clientNum: {}", clientNum);
                    }
                });
    }

    private String getTopicFilter(String actualTopicFilter) {
        if (ordered) {
            return "$oshare/" + "ruleEngine" + "/" + actualTopicFilter;
        }
        return "$share/" + "ruleEngine" + "/" + actualTopicFilter;
    }

    private Message handlePublishedMsg(Mqtt5Publish published) {
        return null;
    }

    private CompletableFuture<Void> closeClients() {
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        clients.forEach(client -> {
            CompletableFuture<Void> future = new CompletableFuture<>();
            futures.add(future);
            client.disconnect().thenAccept(v -> future.complete(null));
        });
        return CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]));
    }
}
