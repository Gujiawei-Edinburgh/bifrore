package bifrore.router.server;

import bifrore.processor.client.IProcessorClient;
import bifrore.processor.rpc.proto.SubscribeResponse;
import bifrore.processor.rpc.proto.UnsubscribeResponse;
import bifrore.router.rpc.proto.AddRuleRequest;
import bifrore.router.rpc.proto.AddRuleResponse;
import bifrore.router.rpc.proto.DeleteRuleRequest;
import bifrore.router.rpc.proto.DeleteRuleResponse;
import bifrore.router.rpc.proto.ListRuleRequest;
import bifrore.router.rpc.proto.ListRuleResponse;
import bifrore.router.rpc.proto.ListTopicFilterRequest;
import bifrore.router.rpc.proto.ListTopicFilterResponse;
import bifrore.router.rpc.proto.MatchRequest;
import bifrore.router.rpc.proto.MatchResponse;
import io.grpc.stub.StreamObserver;
import lombok.SneakyThrows;
import org.mockito.Mock;
import org.mockito.MockitoAnnotations;
import org.testng.annotations.AfterMethod;
import org.testng.annotations.BeforeMethod;
import org.testng.annotations.Test;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;

import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.when;

public class RouterServiceTest {
    AutoCloseable closeable;
    @Mock
    private IProcessorClient processorClient;
    private RouterService routerService;
    private final Map<String, byte[]> idMap = new ConcurrentHashMap<>();
    private final Map<String, List<byte[]>> topicFilterMap = new ConcurrentHashMap<>();
    private final String topicFilter = "a/b/+";
    private final String ruleExpression = "select * from \"a/b/+\" where temp > 40";

    @BeforeMethod
    public void setUp() {
        closeable = MockitoAnnotations.openMocks(this);
        routerService = new RouterService(idMap, topicFilterMap, processorClient);
    }

    @SneakyThrows
    @AfterMethod
    public void clearRuleMap() {
        idMap.clear();
        topicFilterMap.clear();
        closeable.close();
    }


    @Test
    public void testAddRuleOk() {
        AddRuleRequest request = AddRuleRequest.newBuilder()
                .setReqId(1)
                .setRule(ruleExpression)
                .addAllDestinations(new ArrayList<>() {{
                    add("log");
                    add("console");
                }})
                .build();
        when(processorClient.subscribe(any()))
                .thenReturn(CompletableFuture.completedFuture(SubscribeResponse.newBuilder()
                        .setReqId(1)
                        .setCode(SubscribeResponse.Code.OK)
                        .build()));
        AddRuleResponse[] subscribeResponses = new AddRuleResponse[1];
        StreamObserver<AddRuleResponse> subscribeObserver = new StreamObserver<>() {

            @Override
            public void onNext(AddRuleResponse value) {
                subscribeResponses[0] = value;
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.addRule(request, subscribeObserver);
        AddRuleResponse subscribeResponse = subscribeResponses[0];
        assert subscribeResponse.getReqId() == request.getReqId();
        assert subscribeResponse.getCode() == AddRuleResponse.Code.OK;
        assert idMap.containsKey(generateRuleId(ruleExpression));
        assert idMap.size() == 1;
        assert topicFilterMap.containsKey(topicFilter);
        assert topicFilterMap.size() == 1;
    }

    @Test
    public void testAddRuleWithSubFail() {
        AddRuleRequest request = AddRuleRequest.newBuilder()
                .setReqId(1)
                .setRule(ruleExpression)
                .addAllDestinations(new ArrayList<>() {{
                    add("log");
                    add("console");
                }})
                .build();
        when(processorClient.subscribe(any()))
                .thenReturn(CompletableFuture.completedFuture(SubscribeResponse.newBuilder()
                        .setReqId(1)
                        .setCode(SubscribeResponse.Code.ERROR)
                        .setReason("testFailReason")
                        .build()));
        AddRuleResponse[] subscribeResponses = new AddRuleResponse[1];
        StreamObserver<AddRuleResponse> subscribeObserver = new StreamObserver<>() {

            @Override
            public void onNext(AddRuleResponse value) {
                subscribeResponses[0] = value;
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.addRule(request, subscribeObserver);
        AddRuleResponse subscribeResponse = subscribeResponses[0];
        assert subscribeResponse.getReqId() == request.getReqId();
        assert subscribeResponse.getCode() == AddRuleResponse.Code.ERROR;
        assert !idMap.containsKey(generateRuleId(ruleExpression));
        assert idMap.isEmpty();
        assert !topicFilterMap.containsKey(topicFilter);
        assert topicFilterMap.isEmpty();
    }

    @Test
    public void testAddRuleWithSubException() {
        AddRuleRequest request = AddRuleRequest.newBuilder()
                .setReqId(1)
                .setRule(ruleExpression)
                .addAllDestinations(new ArrayList<>() {{
                    add("log");
                    add("console");
                }})
                .build();
        CompletableFuture<SubscribeResponse> future = new CompletableFuture<>();
        future.completeExceptionally(new RuntimeException("testException"));
        when(processorClient.subscribe(any())).thenReturn(future);
        AddRuleResponse[] subscribeResponses = new AddRuleResponse[1];
        StreamObserver<AddRuleResponse> subscribeObserver = new StreamObserver<>() {

            @Override
            public void onNext(AddRuleResponse value) {
                subscribeResponses[0] = value;
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.addRule(request, subscribeObserver);
        AddRuleResponse subscribeResponse = subscribeResponses[0];
        assert subscribeResponse.getReqId() == request.getReqId();
        assert subscribeResponse.getCode() == AddRuleResponse.Code.ERROR;
        assert !idMap.containsKey(generateRuleId(ruleExpression));
        assert idMap.isEmpty();
        assert !topicFilterMap.containsKey(topicFilter);
        assert topicFilterMap.isEmpty();
    }

    @Test
    public void testList() {
        addRule(ruleExpression);
        String anotherRuleExpression = "select * from \"a/b/+\" where height > 10";
        addRule(anotherRuleExpression);
        ListRuleRequest request = ListRuleRequest.newBuilder().setReqId(1).build();
        ListRuleResponse[] listRuleResponses = new ListRuleResponse[1];
        StreamObserver<ListRuleResponse> listObserver = new StreamObserver<>() {

            @Override
            public void onNext(ListRuleResponse value) {
                listRuleResponses[0] = value;
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.listRule(request, listObserver);
        ListRuleResponse listRuleResponse = listRuleResponses[0];
        assert listRuleResponse.getCode() == ListRuleResponse.Code.OK;
        assert idMap.size() == 2;
        assert topicFilterMap.get(topicFilter).size() == 2;
        assert topicFilter.equals(listRuleResponse.getRules(0).getTopicFilter());
        assert topicFilter.equals(listRuleResponse.getRules(1).getTopicFilter());
    }

    @Test
    public void testListRuleWithParseError() {
        idMap.put("a/b/c", "test".getBytes());
        ListRuleRequest request = ListRuleRequest.newBuilder().setReqId(1).build();
        ListRuleResponse[] listRuleResponses = new ListRuleResponse[1];
        StreamObserver<ListRuleResponse> listObserver = new StreamObserver<>() {

            @Override
            public void onNext(ListRuleResponse value) {
                listRuleResponses[0] = value;
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.listRule(request, listObserver);
        ListRuleResponse listRuleResponse = listRuleResponses[0];
        assert listRuleResponse.getCode() == ListRuleResponse.Code.ERROR;
        assert listRuleResponse.getRulesList().isEmpty();
    }

    @Test
    public void testDeleteRuleOk() {
        addRule(ruleExpression);
        DeleteRuleRequest request = DeleteRuleRequest.newBuilder()
                .setReqId(2)
                .setRuleId(generateRuleId(ruleExpression))
                .build();
        when(processorClient.unsubscribe(any()))
                .thenReturn(CompletableFuture.completedFuture(UnsubscribeResponse.newBuilder()
                        .setReqId(2)
                        .setCode(UnsubscribeResponse.Code.OK)
                        .setReason("testFailReason")
                        .build()));
        DeleteRuleResponse[] deleteRuleResponses = new DeleteRuleResponse[1];
        StreamObserver<DeleteRuleResponse> unsubscribeResponseStreamObserver = new StreamObserver<>() {

            @Override
            public void onNext(DeleteRuleResponse value) {
                deleteRuleResponses[0] = value;
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.deleteRule(request, unsubscribeResponseStreamObserver);
        DeleteRuleResponse deleteRuleResponse = deleteRuleResponses[0];
        assert deleteRuleResponse.getReqId() == request.getReqId();
        assert deleteRuleResponse.getCode() == DeleteRuleResponse.Code.OK;
        assert idMap.isEmpty();
        assert topicFilterMap.isEmpty();
    }

    @Test
    public void testDeleteRuleWithUnsubFail() {
        addRule(ruleExpression);
        DeleteRuleRequest request = DeleteRuleRequest.newBuilder()
                .setReqId(2)
                .setRuleId(generateRuleId(ruleExpression))
                .build();
        when(processorClient.unsubscribe(any()))
                .thenReturn(CompletableFuture.completedFuture(UnsubscribeResponse.newBuilder()
                        .setReqId(2)
                        .setCode(UnsubscribeResponse.Code.ERROR)
                        .setReason("testFailReason")
                        .build()));
        DeleteRuleResponse[] deleteRuleResponses = new DeleteRuleResponse[1];
        StreamObserver<DeleteRuleResponse> unsubscribeResponseStreamObserver = new StreamObserver<>() {

            @Override
            public void onNext(DeleteRuleResponse value) {
                deleteRuleResponses[0] = value;
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.deleteRule(request, unsubscribeResponseStreamObserver);
        DeleteRuleResponse deleteRuleResponse = deleteRuleResponses[0];
        assert deleteRuleResponse.getReqId() == request.getReqId();
        assert deleteRuleResponse.getCode() == DeleteRuleResponse.Code.ERROR;
        assert idMap.size() == 1;
        assert topicFilterMap.size() == 1;
    }

    @Test
    public void testDeleteRuleWithUnsubException() {
        addRule(ruleExpression);
        DeleteRuleRequest request = DeleteRuleRequest.newBuilder()
                .setReqId(2)
                .setRuleId(generateRuleId(ruleExpression))
                .build();
        CompletableFuture<UnsubscribeResponse> future = new CompletableFuture<>();
        future.completeExceptionally(new RuntimeException("testException"));
        when(processorClient.unsubscribe(any())).thenReturn(future);
        DeleteRuleResponse[] deleteRuleResponses = new DeleteRuleResponse[1];
        StreamObserver<DeleteRuleResponse> unsubscribeResponseStreamObserver = new StreamObserver<>() {

            @Override
            public void onNext(DeleteRuleResponse value) {
                deleteRuleResponses[0] = value;
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.deleteRule(request, unsubscribeResponseStreamObserver);
        DeleteRuleResponse deleteRuleResponse = deleteRuleResponses[0];
        assert deleteRuleResponse.getReqId() == request.getReqId();
        assert deleteRuleResponse.getCode() == DeleteRuleResponse.Code.ERROR;
        assert idMap.size() == 1;
        assert topicFilterMap.size() == 1;
    }

    @Test
    public void testMatch() {
        addRule(ruleExpression);
        addRule("select * from \"a/#\" where temp > 40");
        MatchRequest request = MatchRequest.newBuilder()
                .setReqId(1)
                .setTopic("a/b/c")
                .build();
        MatchResponse[] matchResponses = new MatchResponse[1];
        StreamObserver<MatchResponse> matchResponseStreamObserver = new StreamObserver<>() {

            @Override
            public void onNext(MatchResponse value) {
                matchResponses[0] = value;
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.match(request, matchResponseStreamObserver);
        MatchResponse matchResponse = matchResponses[0];
        assert matchResponse.getReqId() == 1;
        assert matchResponse.getCode() == MatchResponse.Code.OK;
        assert matchResponse.getParsedRuleInBytesCount() == 2;
    }

    @Test
    public void testMatchWithNoExist() {
        addRule(ruleExpression);
        MatchRequest request = MatchRequest.newBuilder()
                .setReqId(1)
                .setTopic("a/d/c")
                .build();
        MatchResponse[] matchResponses = new MatchResponse[1];
        StreamObserver<MatchResponse> matchResponseStreamObserver = new StreamObserver<>() {

            @Override
            public void onNext(MatchResponse value) {
                matchResponses[0] = value;
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.match(request, matchResponseStreamObserver);
        MatchResponse matchResponse = matchResponses[0];
        assert matchResponse.getReqId() == 1;
        assert matchResponse.getCode() == MatchResponse.Code.NOT_EXIST;
        assert matchResponse.getParsedRuleInBytesCount() == 0;
    }

    @Test
    public void testListTopicFilters() {
        addRule(ruleExpression);
        String anotherRuleExpression = "select * from \"a/b/+\" where height > 10";
        addRule(anotherRuleExpression);
        ListTopicFilterRequest request = ListTopicFilterRequest.newBuilder().setReqId(1).build();
        ListTopicFilterResponse[] responses = new ListTopicFilterResponse[1];
        StreamObserver<ListTopicFilterResponse> streamObserver = new StreamObserver<>() {

            @Override
            public void onNext(ListTopicFilterResponse value) {
                responses[0] = value;
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.listTopiFilters(request, streamObserver);
        ListTopicFilterResponse listTopicFilterResponse = responses[0];
        assert listTopicFilterResponse.getReqId() == request.getReqId();
        assert listTopicFilterResponse.getCode() == ListTopicFilterResponse.Code.OK;
        assert idMap.size() == 2;
        assert topicFilterMap.size() == 1;
    }

    private void addRule(String rule) {
        when(processorClient.subscribe(any()))
                .thenReturn(CompletableFuture.completedFuture(SubscribeResponse.newBuilder()
                        .setReqId(1)
                        .setCode(SubscribeResponse.Code.OK)
                        .build()));
        AddRuleRequest request = AddRuleRequest.newBuilder()
                .setReqId(1)
                .setRule(rule)
                .addAllDestinations(new ArrayList<>() {{
                    add("log");
                    add("console");
                }})
                .build();
        StreamObserver<AddRuleResponse> subscribeObserver = new StreamObserver<>() {

            @Override
            public void onNext(AddRuleResponse value) {
            }

            @Override
            public void onError(Throwable t) {

            }

            @Override
            public void onCompleted() {

            }
        };
        routerService.addRule(request, subscribeObserver);
    }

    private String generateRuleId(String rule) {
        return Integer.toHexString(rule.hashCode());
    }
}
