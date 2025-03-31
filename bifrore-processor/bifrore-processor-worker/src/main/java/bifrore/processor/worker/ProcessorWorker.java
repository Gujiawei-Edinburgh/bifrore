package bifrore.processor.worker;

import bifrore.common.parser.RuleEvaluator;
import bifrore.commontype.Message;
import bifrore.commontype.QoS;
import bifrore.destination.plugin.ProducerManager;
import bifrore.router.client.IRouterClient;
import bifrore.router.client.Matched;
import bifrore.router.rpc.proto.MatchRequest;
import com.github.benmanes.caffeine.cache.Caffeine;
import com.github.benmanes.caffeine.cache.LoadingCache;
import com.google.protobuf.ByteString;
import com.hivemq.client.mqtt.MqttClientTransportConfig;
import com.hivemq.client.mqtt.mqtt5.Mqtt5AsyncClient;
import com.hivemq.client.mqtt.mqtt5.Mqtt5Client;
import com.hivemq.client.mqtt.mqtt5.message.auth.Mqtt5SimpleAuth;
import com.hivemq.client.mqtt.mqtt5.message.connect.Mqtt5Connect;
import com.hivemq.client.mqtt.mqtt5.message.connect.connack.Mqtt5ConnAckReasonCode;
import com.hivemq.client.mqtt.mqtt5.message.publish.Mqtt5Publish;
import com.hivemq.client.mqtt.mqtt5.message.subscribe.Mqtt5Subscribe;
import com.hivemq.client.mqtt.mqtt5.message.unsubscribe.Mqtt5Unsubscribe;
import lombok.extern.slf4j.Slf4j;

import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;

@Slf4j
class ProcessorWorker implements IProcessorWorker {
    private final int clientNum;
    private final String groupName;
    private final String userName;
    private final String password;
    private final boolean cleanStart;
    private final long sessionExpiryInterval;
    private final String host;
    private final int port;
    private final String clientPrefix;
    private final RuleEvaluator ruleEvaluator;
    private final ProducerManager producerManager;
    private final IRouterClient routerClient;
    private final List<Mqtt5AsyncClient> clients = new ArrayList<>();
    private final LoadingCache<String, CompletableFuture<List<Matched>>> matchedRules;

    ProcessorWorker(ProcessorWorkerBuilder builder) {
        clientNum = builder.clientNum;
        groupName = builder.groupName;
        userName = builder.userName;
        password = builder.password;
        cleanStart = builder.cleanStart;
        sessionExpiryInterval = builder.sessionExpiryInterval;
        host = builder.host;
        port = builder.port;
        clientPrefix = builder.clientPrefix + "/" + builder.nodeId;
        producerManager = new ProducerManager(builder.pluginManager, builder.callerCfgs);
        routerClient = builder.routerClient;
        matchedRules = Caffeine.newBuilder()
                .maximumSize(100)
                .build(topic -> routerClient.match(MatchRequest.newBuilder().setTopic(topic).build()));
        ruleEvaluator = new RuleEvaluator();
    }

    @Override
    public void start() {
        initProcessorWorker();
    }

    @Override
    public CompletableFuture<Void> sub(String topicFilter) {
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        if (clients.isEmpty()) {
            return handleEmptyClientList();
        }
        clients.forEach(asyncClient -> {
            CompletableFuture<Void> future = new CompletableFuture<>();
            futures.add(future);
            Mqtt5Subscribe mqtt5Subscribe = Mqtt5Subscribe.builder()
                    .topicFilter(convertToSharedSubscription(topicFilter))
                    .noLocal(false)
                    .build();
            asyncClient.subscribe(mqtt5Subscribe, this::handlePublishedMsg, true)
                    .whenComplete(((mqtt5SubAck, error) -> {
                        if (error != null) {
                            future.completeExceptionally(error);
                        }else {
                            future.complete(null);
                        }
                    }));
        });
        return CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]))
                .whenComplete((v,e) -> {
                    if (e != null) {
                        log.error("Failed to subscribe topicFilter {}: ", topicFilter, e);
                    }
                });
    }

    @Override
    public CompletableFuture<Void> unsub(String topicFilter) {
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        if (clients.isEmpty()) {
            return handleEmptyClientList();
        }
        clients.forEach(asyncClient -> {
            CompletableFuture<Void> future = new CompletableFuture<>();
            futures.add(future);
            asyncClient.unsubscribe(Mqtt5Unsubscribe.builder()
                            .topicFilter(convertToSharedSubscription(topicFilter))
                            .build())
                    .whenComplete(((mqtt5UnsubAck, error) -> {
                        if (error != null) {
                            future.completeExceptionally(error);
                        }else {
                            future.complete(null);
                        }
                    }));
        });
        return CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]))
                .whenComplete((v,e) -> {
                    if (e != null) {
                        log.error("Failed to unsubscribe topicFilter: {}, ", topicFilter, e);
                    }
                });
    }

    @Override
    public CompletableFuture<String> addDestination(String destinationType, Map<String, String> destinationCfg) {
        return producerManager.createDestinationCaller(destinationType, destinationCfg);
    }

    @Override
    public void close() {
        closeClients().thenAccept(v -> {
            clients.clear();
            producerManager.close();
        });
    }

    private void initProcessorWorker() {
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        routerClient.listTopicFilter().thenAccept(listTopicFilterResponse -> {
            for (int idx = 0; idx < clientNum; idx++) {
                String identifier = clientPrefix + "/" + idx;
                log.info("init client, host: {}, port: {}", host, port);
                Mqtt5AsyncClient asyncClient = Mqtt5Client.builder()
                        .identifier(identifier)
                        .transportConfig(MqttClientTransportConfig.builder()
                                .serverHost(host)
                                .serverPort(port)
                                .mqttConnectTimeout(10, TimeUnit.SECONDS)
                                .socketConnectTimeout(10, TimeUnit.SECONDS)
                                .build())
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
                asyncClient.connect(mqtt5Connect).whenComplete((mqtt5ConnAck, conErr) -> {
                    if (conErr != null) {
                        log.error("ClientId: {} connect to broker failed, ", identifier, conErr);
                        future.completeExceptionally(conErr);
                    } else if (mqtt5ConnAck.getReasonCode() != Mqtt5ConnAckReasonCode.SUCCESS) {
                        log.error("ClientId: {} connect to broker failed: {}", identifier,
                                mqtt5ConnAck.getReasonCode());
                        future.completeExceptionally(new RuntimeException(mqtt5ConnAck.getReasonCode().toString()));
                    } else {
                        clients.add(asyncClient);
                        List<CompletableFuture<Void>> subscribeFutures = new ArrayList<>();
                        listTopicFilterResponse.getTopicFiltersList().forEach(topicFilter -> {
                            CompletableFuture<Void> subscribeFuture = new CompletableFuture<>();
                            subscribeFutures.add(subscribeFuture);
                            Mqtt5Subscribe mqtt5Subscribe = Mqtt5Subscribe.builder()
                                    .topicFilter(convertToSharedSubscription(topicFilter))
                                    .noLocal(false)
                                    .build();
                            asyncClient.subscribe(mqtt5Subscribe, this::handlePublishedMsg)
                                    .whenComplete(((mqtt5SubAck, error) -> {
                                        if (error != null) {
                                            log.error("Failed to subscribe: {}", mqtt5Subscribe);
                                            subscribeFuture.completeExceptionally(
                                                    new RuntimeException("Init client during subscription", error)
                                            );
                                            clients.remove(asyncClient);
                                        }else {
                                            subscribeFuture.complete(null);
                                        }
                                    }));
                        });
                        CompletableFuture.allOf(subscribeFutures.toArray(new CompletableFuture[0]))
                                .whenComplete((v, e) -> {
                                    if (e != null) {
                                        future.completeExceptionally(e);
                                    }else {
                                        future.complete(null);
                                    }
                                });
                    }
                });
            }
            CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]))
                    .whenComplete((v,e) -> {
                        if (e != null) {
                            log.error("Failed to init processor worker: ", e);
                        }else {
                            log.info("Init processor worker ok, clientNum: {}", clientNum);
                        }
                    });
        });
    }

    private void handlePublishedMsg(Mqtt5Publish published) {
        Message message = Message.newBuilder()
                .setQos(QoS.forNumber(published.getQos().getCode()))
                .setTopic(published.getTopic().toString())
                .setPayload(ByteString.copyFrom(published.getPayloadAsBytes()))
                .build();
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        matchedRules.get(message.getTopic())
                .whenComplete((matchedList, e) -> {
                    CompletableFuture<Void> future = new CompletableFuture<>();
                    futures.add(future);
                    if (e != null) {
                        log.error("Failed to get matched rules: {}", published);
                        future.completeExceptionally(e);
                    }else {
                        matchedList.forEach(matched -> ruleEvaluator.evaluate(matched.parsed(), message).
                                ifPresent(value -> producerManager.produce(matched.destinations(), value)
                                        .whenComplete((v, ex) -> {
                                            if (ex != null) {
                                                log.error("Failed to send message to producers: {}", published, ex);
                                                future.completeExceptionally(ex);
                                            }else {
                                                future.complete(null);
                                            }
                                })));
                    }
                });
        CompletableFuture.allOf(futures.toArray(new CompletableFuture[0])).whenComplete((v,e) -> {
            if (e != null) {
                log.error("Failed to handle published message: {}", published);
            }else {
                published.acknowledge();
            }
        });
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

    private String convertToSharedSubscription(String topicFilter) {
        return "$share/" + groupName + "/" + topicFilter;
    }

    private CompletableFuture<Void> handleEmptyClientList() {
        CompletableFuture<Void> future = new CompletableFuture<>();
        future.completeExceptionally(new RuntimeException("No available client found"));
        return future;
    }
}
