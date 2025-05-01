package bifrore.router.server;

import bifrore.common.parser.ParsedRule;
import bifrore.common.parser.util.ParsedRuleHelper;
import bifrore.common.parser.util.ParsedSerializeUtil;
import bifrore.commontype.QoS;
import bifrore.monitoring.metrics.SysMeter;
import bifrore.monitoring.metrics.SysMetric;
import bifrore.processor.client.IProcessorClient;
import bifrore.processor.rpc.proto.SubscribeRequest;
import bifrore.processor.rpc.proto.SubscribeResponse;
import bifrore.processor.rpc.proto.UnsubscribeRequest;
import bifrore.processor.rpc.proto.UnsubscribeResponse;

import bifrore.router.rpc.proto.AddRuleRequest;
import bifrore.router.rpc.proto.AddRuleResponse;
import bifrore.router.rpc.proto.CompiledRule;
import bifrore.router.rpc.proto.DeleteRuleRequest;
import bifrore.router.rpc.proto.DeleteRuleResponse;
import bifrore.router.rpc.proto.ListRuleRequest;
import bifrore.router.rpc.proto.ListRuleResponse;
import bifrore.router.rpc.proto.ListTopicFilterRequest;
import bifrore.router.rpc.proto.ListTopicFilterResponse;
import bifrore.router.rpc.proto.MatchRequest;
import bifrore.router.rpc.proto.MatchResponse;
import bifrore.router.rpc.proto.RouterServiceGrpc;
import bifrore.router.rpc.proto.RuleMeta;
import bifrore.router.server.util.TopicFilterUtil;
import com.google.protobuf.ByteString;
import com.google.protobuf.InvalidProtocolBufferException;
import io.grpc.stub.StreamObserver;
import io.micrometer.core.instrument.Timer;
import lombok.extern.slf4j.Slf4j;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.stream.Collectors;

import static bifrore.baserpc.UnaryResponse.response;
import static bifrore.monitoring.metrics.SysMetric.AddRuleLatency;
import static bifrore.monitoring.metrics.SysMetric.ParsingRuleLatency;
import static bifrore.monitoring.metrics.SysMetric.RuleNumGauge;

@Slf4j
public class RouterService extends RouterServiceGrpc.RouterServiceImplBase {
    private final Map<String, byte[]> idMap;
    private final Map<String, List<byte[]>> topicFilterMap;
    private final IProcessorClient processorClient;

    public RouterService(Map<String, byte[]> idMap,
                         Map<String, List<byte[]>> topicFilterMap,
                         IProcessorClient processorClient) {
        this.idMap = idMap;
        this.topicFilterMap = topicFilterMap;
        this.processorClient = processorClient;
    }

    public void start() {
        SysMeter.INSTANCE.startGauge(RuleNumGauge, idMap::size);
    }

    @Override
    public void match(MatchRequest request, StreamObserver<MatchResponse> responseObserver) {
        response(metadata -> {
            CompletableFuture<MatchResponse> future = new CompletableFuture<>();
            MatchResponse.Builder builder = MatchResponse.newBuilder();
            String topic = request.getTopic();
            builder.setReqId(request.getReqId());
            List<ByteString> matched = topicFilterMap.entrySet().
                    stream()
                    .filter(entry -> TopicFilterUtil.isMatch(topic, entry.getKey()))
                    .flatMap(entry -> entry.getValue().stream())
                    .map(ByteString::copyFrom)
                    .toList();
            if (matched.isEmpty()) {
                builder.setCode(MatchResponse.Code.NOT_EXIST);
            }else {
                builder.setCode(MatchResponse.Code.OK);
            }
            builder.addAllParsedRuleInBytes(matched);
            future.complete(builder.build());
            return future;
        }, responseObserver);
    }

    @Override
    public void addRule(AddRuleRequest request, StreamObserver<AddRuleResponse> responseObserver) {
        response(metadata -> {
            Timer.Sample addRuleSampler = Timer.start();
            CompletableFuture<AddRuleResponse> future = new CompletableFuture<>();
            AddRuleResponse.Builder builder = AddRuleResponse.newBuilder();
            builder.setReqId(request.getReqId());
            try {
                Timer.Sample parsingSampler = Timer.start();
                ParsedRule parsedRule = ParsedRuleHelper.getInstance(request.getRule());
                parsingSampler.stop(SysMeter.INSTANCE.timer(ParsingRuleLatency));
                String topicFilter = parsedRule.getTopicFilter();
                processorClient.subscribe(SubscribeRequest.newBuilder()
                        .setReqId(request.getReqId())
                        .setTopicFilter(topicFilter)
                        .setQos(QoS.AT_LEAST_ONCE)
                        .build()).whenComplete((v, e) -> {
                    if (e != null || v.getCode() == SubscribeResponse.Code.ERROR) {
                        builder.setCode(AddRuleResponse.Code.ERROR);
                        String failReason;
                        if (e != null) {
                            failReason = e.getMessage();
                        }else {
                            failReason = v.getReason();
                        }
                        builder.setFailReason(failReason);
                    }else {
                        try {
                            byte[] serializedParsed = ParsedSerializeUtil.serializeParsed(parsedRule.getParsed());
                            String ruleId = generateRuleId(request.getRule());
                            idMap.putIfAbsent(ruleId, RuleMeta.newBuilder()
                                    .setRuleId(ruleId)
                                    .setPlaintextRule(request.getRule())
                                    .setTopicFilter(topicFilter)
                                    .addAllDestinations(request.getDestinationsList())
                                    .build().toByteArray());
                            CompiledRule compiledRule = CompiledRule.newBuilder()
                                    .setRuleId(ruleId)
                                    .setExpressionObj(ByteString.copyFrom(serializedParsed))
                                    .addAllDestinations(request.getDestinationsList())
                                    .build();
                            while (true) {
                                List<byte[]> existingRules = topicFilterMap.get(topicFilter);
                                List<byte[]> updatedRules;
                                if (existingRules == null) {
                                    updatedRules = new ArrayList<>();
                                } else {
                                    updatedRules = new ArrayList<>(existingRules);
                                }
                                updatedRules.add(compiledRule.toByteArray());
                                if (existingRules == null) {
                                    if (topicFilterMap.putIfAbsent(topicFilter, updatedRules) == null) {
                                        break;
                                    }
                                }else {
                                    if (topicFilterMap.replace(topicFilter, existingRules, updatedRules)) {
                                        break;
                                    }
                                }
                            }
                            builder.setRuleId(ruleId);
                            builder.setCode(AddRuleResponse.Code.OK);
                        }catch (IOException exception) {
                            builder.setCode(AddRuleResponse.Code.ERROR);
                            builder.setFailReason(exception.getMessage());
                        }
                    }
                    future.complete(builder.build());
                    addRuleSampler.stop(SysMeter.INSTANCE.timer(AddRuleLatency));
                });
            } catch (Exception e) {
                log.error("Subscribe failed", e);
                builder.setCode(AddRuleResponse.Code.ERROR);
                builder.setFailReason(e.getMessage());
                future.complete(builder.build());
            }
            return future;
        }, responseObserver);
    }

    @Override
    public void deleteRule(DeleteRuleRequest request, StreamObserver<DeleteRuleResponse> responseObserver) {
        response(metadata -> {
            CompletableFuture<DeleteRuleResponse> future = new CompletableFuture<>();
            DeleteRuleResponse.Builder builder = DeleteRuleResponse.newBuilder();
            builder.setReqId(request.getReqId());
            byte[] deleted = idMap.remove(request.getRuleId());
            if (deleted == null) {
                builder.setCode(DeleteRuleResponse.Code.OK);
                future.complete(builder.build());
                return future;
            }
            try {
                RuleMeta ruleMeta = RuleMeta.parseFrom(deleted);
                String topicFilter = ruleMeta.getTopicFilter();
                boolean emptyRules = false;
                while (true) {
                    List<byte[]> existingRules = topicFilterMap.get(topicFilter);
                    if (existingRules == null || existingRules.isEmpty()) {
                        break;
                    }
                    List<byte[]> updatedRules = new ArrayList<>(existingRules);
                    updatedRules.removeIf(compiledRuleBytes -> {
                        try {
                            CompiledRule compiledRule = CompiledRule.parseFrom(compiledRuleBytes);
                            return compiledRule.getRuleId().equals(request.getRuleId());
                        } catch (InvalidProtocolBufferException ex) {
                            return false;
                        }
                    });
                    if (updatedRules.isEmpty()) {
                        topicFilterMap.remove(topicFilter);
                        emptyRules = true;
                        break;
                    }else {
                        if (topicFilterMap.replace(topicFilter, existingRules, updatedRules)) {
                            break;
                        }
                    }
                }
                if (emptyRules) {
                    UnsubscribeRequest unsubscribeRequest = UnsubscribeRequest.newBuilder()
                            .setReqId(request.getReqId())
                            .setTopicFilter(topicFilter)
                            .build();
                    processorClient.unsubscribe(unsubscribeRequest)
                            .whenComplete((v, e) -> {
                                if (e != null || v.getCode() == UnsubscribeResponse.Code.ERROR) {
                                    builder.setCode(DeleteRuleResponse.Code.ERROR);
                                    String failReason;
                                    if (e != null) {
                                        failReason = e.getMessage();
                                    }else {
                                        failReason = v.getReason();
                                    }
                                    builder.setFailReason(failReason);
                                }else {
                                    builder.setCode(DeleteRuleResponse.Code.OK);
                                }
                                future.complete(builder.build());
                            });
                } else {
                    builder.setCode(DeleteRuleResponse.Code.OK);
                    future.complete(builder.build());
                }
            } catch (InvalidProtocolBufferException ex) {
                builder.setCode(DeleteRuleResponse.Code.ERROR);
                builder.setFailReason(ex.getMessage());
                future.complete(builder.build());
            }
            return future;
        }, responseObserver);
    }

    @Override
    public void listRule(ListRuleRequest request, StreamObserver<ListRuleResponse> responseObserver) {
        response(metadata -> {
            CompletableFuture<ListRuleResponse> future = new CompletableFuture<>();
            ListRuleResponse.Builder builder = ListRuleResponse.newBuilder();
            builder.setReqId(request.getReqId());
            try {
                List<RuleMeta> rules = idMap.values().stream()
                        .map(bytes -> {
                            try {
                                return parseRuleMeta(bytes);
                            }catch (InvalidProtocolBufferException exception) {
                                throw new RuntimeException(exception);
                            }
                        })
                        .collect(Collectors.toList());

                builder.setCode(ListRuleResponse.Code.OK);
                builder.addAllRules(rules);
                future.complete(builder.build());
            } catch (RuntimeException e) {
                log.error("Failed to parse processedRule", e);
                builder.setCode(ListRuleResponse.Code.ERROR);
                builder.setFailReason(e.getMessage());
                future.complete(builder.build());
            }

            return future;
        }, responseObserver);
    }

    @Override
    public void listTopiFilters(ListTopicFilterRequest request, StreamObserver<ListTopicFilterResponse> responseObserver) {
        response(metadata -> {
            CompletableFuture<ListTopicFilterResponse> future = new CompletableFuture<>();
            ListTopicFilterResponse.Builder builder = ListTopicFilterResponse.newBuilder();
            builder.setReqId(request.getReqId());
            builder.addAllTopicFilters(topicFilterMap.keySet());
            future.complete(builder.build());
            return future;
        }, responseObserver);
    }

    public void stop() {
        SysMeter.INSTANCE.stopGauge(RuleNumGauge);
    }

    private RuleMeta parseRuleMeta(byte[] ruleMetaInBytes) throws InvalidProtocolBufferException {
        return RuleMeta.parseFrom(ruleMetaInBytes);
    }

    private String generateRuleId(String rule) {
        return Integer.toHexString(rule.hashCode());
    }
}
