package bifrore.processor.worker;

import bifrore.common.parser.RuleEvaluator;
import bifrore.commontype.MapMessage;
import bifrore.commontype.Message;
import bifrore.commontype.QoS;
import bifrore.destination.plugin.ProducerManager;
import bifrore.monitoring.metrics.SysMeter;
import bifrore.router.client.IRouterClient;
import bifrore.router.client.Matched;
import bifrore.router.rpc.proto.MatchRequest;
import com.github.benmanes.caffeine.cache.AsyncLoadingCache;
import com.github.benmanes.caffeine.cache.Caffeine;
import com.github.benmanes.caffeine.cache.Scheduler;
import com.google.protobuf.ByteString;
import com.hivemq.client.mqtt.MqttClientExecutorConfig;
import com.hivemq.client.mqtt.MqttClientTransportConfig;
import com.hivemq.client.mqtt.mqtt5.Mqtt5AsyncClient;
import com.hivemq.client.mqtt.mqtt5.Mqtt5Client;
import com.hivemq.client.mqtt.mqtt5.message.auth.Mqtt5SimpleAuth;
import com.hivemq.client.mqtt.mqtt5.message.connect.Mqtt5Connect;
import com.hivemq.client.mqtt.mqtt5.message.connect.connack.Mqtt5ConnAckReasonCode;
import com.hivemq.client.mqtt.mqtt5.message.publish.Mqtt5Publish;
import com.hivemq.client.mqtt.mqtt5.message.subscribe.Mqtt5Subscribe;
import com.hivemq.client.mqtt.mqtt5.message.unsubscribe.Mqtt5Unsubscribe;
import io.micrometer.core.instrument.Metrics;
import io.micrometer.core.instrument.Timer;
import io.micrometer.core.instrument.binder.jvm.ExecutorServiceMetrics;
import io.reactivex.schedulers.Schedulers;
import lombok.extern.slf4j.Slf4j;

import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.ForkJoinPool;
import java.util.concurrent.ForkJoinWorkerThread;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Consumer;

import static bifrore.monitoring.metrics.SysMetric.CachedTopicGauge;
import static bifrore.monitoring.metrics.SysMetric.HandleMessageLatency;
import static bifrore.monitoring.metrics.SysMetric.MatchRuleLatency;
import static bifrore.monitoring.metrics.SysMetric.ProcessorInboundCount;
import static bifrore.monitoring.metrics.SysMetric.RuleNumGauge;

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
    private final boolean ordered;
    private final String orderedTopicFilterPrefix;
    private final RuleEvaluator ruleEvaluator;
    private final ProducerManager producerManager;
    private final IRouterClient routerClient;
    private final List<Mqtt5AsyncClient> clients = new ArrayList<>();
    private final AsyncLoadingCache<String, List<Matched>> matchedRuleCache;
    private final ExecutorService matchExecutor;
    private final TaskTracker taskTracker = new TaskTracker();

    class PublishMessageConsumer implements Consumer<Mqtt5Publish> {
        private final List<Optional<List<Matched>>> rob;
        private final Map<Mqtt5Publish, Integer> robEntryIndex;

        PublishMessageConsumer() {
            this.rob = new LinkedList<>();
            this.robEntryIndex = new HashMap<>();
        }

        @Override
        public void accept(Mqtt5Publish published) {
            SysMeter.INSTANCE.recordCount(ProcessorInboundCount);
            Timer.Sample handleSampler = Timer.start();
            this.rob.add(Optional.empty());
            robEntryIndex.put(published, rob.size() - 1);
            taskTracker.track(published);
            Timer.Sample matchSampler = Timer.start();
            matchedRuleCache.get(published.getTopic().toString())
                    .whenComplete((matchedList, e) -> {
                        matchSampler.stop(SysMeter.INSTANCE.timer(MatchRuleLatency));
                        CompletableFuture<Void> matchFuture = taskTracker.getFutures(published).get(0);
                        if (e != null) {
                            log.error("Failed to get matched rules: {}", published);
                            matchFuture.completeExceptionally(e);
                            matchedList = List.of();
                        }else {
                            matchFuture.complete(null);
                        }
                        int idx = this.robEntryIndex.get(published);
                        rob.set(idx, Optional.of(matchedList));
                        if (idx == 0) {
                            rob.remove(0);
                            robEntryIndex.remove(published);
                            fireMatchedList(matchedList, published);
                            Iterator<Optional<List<Matched>>> itr = this.rob.iterator();
                            while (itr.hasNext()) {
                                Optional<List<Matched>> robResult = itr.next();
                                if (robResult.isPresent()) {
                                    fireMatchedList(robResult.get(), published);
                                    itr.remove();
                                } else {
                                    break;
                                }
                            }
                        }
                    });
            CompletableFuture.allOf(taskTracker.getFutures(published).toArray(new CompletableFuture[0]))
                    .whenComplete((v,e) -> {
                        handleSampler.stop(SysMeter.INSTANCE.timer(HandleMessageLatency));
                        if (e != null) {
                            log.error("Failed to handle published message: {}", published);
                        }else {
                            published.acknowledge();
                        }
                    });
        }
    }

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
        ordered = builder.ordered;
        orderedTopicFilterPrefix = builder.orderedTopicFilterPrefix;
        producerManager = new ProducerManager(builder.pluginManager, builder.callerCfgs);
        routerClient = builder.routerClient;
        matchExecutor = ExecutorServiceMetrics.monitor(Metrics.globalRegistry,
                new ForkJoinPool(2, new ForkJoinPool.ForkJoinWorkerThreadFactory() {
                    final AtomicInteger index = new AtomicInteger(0);

                    @Override
                    public ForkJoinWorkerThread newThread(ForkJoinPool pool) {
                        ForkJoinWorkerThread worker = ForkJoinPool.defaultForkJoinWorkerThreadFactory.newThread(pool);
                        worker.setName(String.format("topic-matcher-%d", index.incrementAndGet()));
                        worker.setDaemon(false);
                        return worker;
                    }
                }, null, false), "topic-matcher");
        matchedRuleCache = Caffeine.newBuilder()
                .scheduler(Scheduler.systemScheduler())
                .maximumSize(100)
                .expireAfterAccess(Duration.ofSeconds(30))
                .expireAfterWrite(Duration.ofSeconds(60))
                .executor(matchExecutor)
                .buildAsync((key, executor) ->
                        routerClient.match(MatchRequest.newBuilder().setTopic(key).build())
                );
        SysMeter.INSTANCE.startGauge(RuleNumGauge, matchedRuleCache.synchronous()::estimatedSize);
        ruleEvaluator = new RuleEvaluator();
    }

    @Override
    public void start() {
        initProcessorWorker();
        producerManager.start();
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
            asyncClient.subscribe(mqtt5Subscribe, new PublishMessageConsumer(), true)
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
    public CompletableFuture<Void> removeDestination(String destinationId) {
        return producerManager.deleteDestinationCaller(destinationId);
    }

    @Override
    public CompletableFuture<Map<String, MapMessage>> listDestinations() {
        return CompletableFuture.completedFuture(producerManager.listAllDestinations());
    }

    @Override
    public void stop() {
        closeClients().thenAccept(v -> {
            clients.clear();
            producerManager.stop();
        }).join();
        matchExecutor.shutdown();
        SysMeter.INSTANCE.stopGauge(CachedTopicGauge);
    }

    private void initProcessorWorker() {
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        routerClient.listTopicFilter().whenComplete((listTopicFilterResponse, throwable) -> {
            if (throwable != null) {
                log.error("Failed to list topicFilter", throwable);
            } else {
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
                            .automaticReconnectWithDefaultConfig()
                            .executorConfig(MqttClientExecutorConfig.builder()
                                    .applicationScheduler(Schedulers.single())
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
                                asyncClient.subscribe(mqtt5Subscribe, new PublishMessageConsumer(), true)
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
            }
        });
    }

    private void fireMatchedList(List<Matched> matchedList, Mqtt5Publish message) {
        Message.Builder messageBuilder = Message.newBuilder()
                .setQos(QoS.forNumber(message.getQos().getCode()))
                .setTopic(message.getTopic().toString())
                .setPayload(ByteString.copyFrom(message.getPayloadAsBytes()));
        CompletableFuture<Void> produceFuture = taskTracker.getFutures(message).get(1);
        if (matchedList == null || matchedList.isEmpty()) {
            produceFuture.complete(null);
        }
        List<CompletableFuture<Void>> futures = new ArrayList<>();
        matchedList.forEach(matched -> {
	       	messageBuilder.setTopicFilter(matched.aliasedTopicFilter());
                ruleEvaluator.evaluate(matched.parsed(), messageBuilder).
			ifPresent(value -> futures.add( producerManager.produce(matched.destinations(), value)));
        });
        CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]))
                .whenComplete((v, e) -> {
                   if (e != null) {
                       log.error("Failed to produce published message: {}", message, e);
                       produceFuture.completeExceptionally(e);
                   } else {
                       produceFuture.complete(null);
                   }
                });
    }

    private CompletableFuture<Void> closeClients() {
        CompletableFuture<Void> closeFuture = new CompletableFuture<>();
        routerClient.listTopicFilter().whenComplete((response, e) -> {
            if (e != null) {
                log.error("Failed to list topic filter", e);
                closeFuture.completeExceptionally(e);
            }else {
                List<CompletableFuture<Void>> futures = new ArrayList<>();
                for (Mqtt5AsyncClient client : clients) {
                    for (String topicFilter: response.getTopicFiltersList()) {
                        CompletableFuture<Void> future = new CompletableFuture<>();
                        futures.add(future);
                        client.unsubscribe(Mqtt5Unsubscribe.builder()
                                .topicFilter(convertToSharedSubscription(topicFilter))
                                .build())
                                .whenComplete(((mqtt5UnsubAck, error) -> {
                                    if (error != null) {
                                        future.completeExceptionally(error);
                                    }else {
                                        future.complete(null);
                                    }
                                }));
                    }
                }
                CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]))
                        .whenComplete((v, ex) -> {
                            if (ex != null) {
                                log.error("Failed to unsubscribe topicFilters", ex);
                                closeFuture.completeExceptionally(ex);
                            }else {
                                closeFuture.complete(null);
                            }
                        });
            }
        });
        return closeFuture;
    }

    private String convertToSharedSubscription(String topicFilter) {
        if (!ordered) {
            return "$share/" + groupName + "/" + topicFilter;
        }else {
            return orderedTopicFilterPrefix + "/" + groupName + "/" + topicFilter;
        }
    }

    private CompletableFuture<Void> handleEmptyClientList() {
        CompletableFuture<Void> future = new CompletableFuture<>();
        future.completeExceptionally(new RuntimeException("No available client found"));
        return future;
    }
}
