package bifromq.re.router.client;

import bifromq.re.common.parser.Parsed;
import bifromq.re.router.rpc.proto.*;

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
