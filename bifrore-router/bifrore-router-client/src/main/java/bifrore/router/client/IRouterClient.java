package bifrore.router.client;

import bifrore.router.rpc.proto.AddRuleRequest;
import bifrore.router.rpc.proto.AddRuleResponse;
import bifrore.router.rpc.proto.DeleteRuleRequest;
import bifrore.router.rpc.proto.DeleteRuleResponse;
import bifrore.router.rpc.proto.ListRuleRequest;
import bifrore.router.rpc.proto.ListRuleResponse;
import bifrore.router.rpc.proto.ListTopicFilterResponse;
import bifrore.router.rpc.proto.MatchRequest;

import java.util.List;
import java.util.concurrent.CompletableFuture;

public interface IRouterClient {
    static RouterClientBuilder newBuilder() {
        return new RouterClientBuilder();
    }

    CompletableFuture<List<Matched>> match(MatchRequest request);

    CompletableFuture<AddRuleResponse> addRule(AddRuleRequest request);

    CompletableFuture<DeleteRuleResponse> deleteRule(DeleteRuleRequest request);

    CompletableFuture<ListRuleResponse> listRule(ListRuleRequest request);

    CompletableFuture<ListTopicFilterResponse> listTopicFilter();
}
