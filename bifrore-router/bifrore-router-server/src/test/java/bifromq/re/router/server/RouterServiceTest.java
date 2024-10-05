package bifromq.re.router.server;

import bifromq.re.router.rpc.proto.*;
import io.grpc.stub.StreamObserver;
import org.testng.annotations.AfterMethod;
import org.testng.annotations.Test;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

public class RouterServiceTest {
    private final Map<String, byte[]> idMap = new ConcurrentHashMap<>();
    private final Map<String, List<byte[]>> topicFilterMap = new ConcurrentHashMap<>();
    private final RouterService routerService = new RouterService(idMap, topicFilterMap);
    private final String topicFilter = "a/b/+";
    private final String ruleExpression = "select * from \"a/b/+\" where temp > 40";

    @AfterMethod
    public void clearRuleMap() {
        idMap.clear();
        topicFilterMap.clear();
    }


    @Test
    public void testAddRule() {
        AddRuleRequest request = AddRuleRequest.newBuilder()
                .setReqId(1)
                .setRule(ruleExpression)
                .addAllDestinations(new ArrayList<>() {{
                    add("log");
                    add("console");
                }})
                .build();
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
    public void testDeleteRule() {
        addRule(ruleExpression);
        DeleteRuleRequest request = DeleteRuleRequest.newBuilder()
                .setReqId(2)
                .setRuleId(generateRuleId(ruleExpression))
                .build();
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
    public void testDeleteWithNoExist() {
        DeleteRuleRequest request = DeleteRuleRequest.newBuilder()
                .setReqId(2)
                .setRuleId(generateRuleId(ruleExpression))
                .build();
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
        assert deleteRuleResponse.getCode() == DeleteRuleResponse.Code.NOT_EXIST;
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

    private void addRule(String rule) {
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
