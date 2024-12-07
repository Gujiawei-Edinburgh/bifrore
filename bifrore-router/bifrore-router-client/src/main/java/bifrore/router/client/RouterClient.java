package bifrore.router.client;

import bifrore.baserpc.IRPCClient;
import bifrore.common.parser.Parsed;
import bifrore.common.parser.util.SerializeUtil;
import bifrore.router.rpc.proto.*;
import lombok.extern.slf4j.Slf4j;

import java.util.Collections;
import java.util.List;
import java.util.Objects;
import java.util.concurrent.CompletableFuture;
import java.util.stream.Collectors;

@Slf4j
final class RouterClient implements IRouterClient {
    private final IRPCClient rpcClient;

    RouterClient(IRPCClient rpcClient) {
        this.rpcClient = rpcClient;
    }

    @Override
    public CompletableFuture<List<Matched>> match(MatchRequest request) {
        CompletableFuture<MatchResponse> future = rpcClient.invoke(request, Collections.emptyMap(),
                RouterServiceGrpc.getMatchMethod());
        return future.thenApply(matchResponse -> {
            if (matchResponse.getCode() != MatchResponse.Code.OK) {
                return List.of();
            }
            return matchResponse.getParsedRuleInBytesList().stream()
                    .map(bytes -> {
                        try {
                            CompiledRule compiledRule = CompiledRule.parseFrom(bytes);
                            Parsed parsed = SerializeUtil.deserializeParsed(compiledRule
                                    .getExpressionObj().toByteArray());
                            return new Matched(parsed, compiledRule.getDestinationsList());
                        } catch (Exception e) {
                            return null;
                        }
                    })
                    .filter(Objects::nonNull)
                    .collect(Collectors.toList());
        });
    }

    @Override
    public CompletableFuture<AddRuleResponse> addRule(AddRuleRequest request) {
        return rpcClient.invoke(request, Collections.emptyMap(), RouterServiceGrpc.getAddRuleMethod());
    }

    @Override
    public CompletableFuture<DeleteRuleResponse> deleteRule(DeleteRuleRequest request) {
        return rpcClient.invoke(request, Collections.emptyMap(), RouterServiceGrpc.getDeleteRuleMethod());
    }

    @Override
    public CompletableFuture<ListRuleResponse> listRule(ListRuleRequest request) {
        return rpcClient.invoke(request, Collections.emptyMap(), RouterServiceGrpc.getListRuleMethod());
    }

    @Override
    public CompletableFuture<ListTopicFilterResponse> listTopicFilter() {
        return rpcClient.invoke(ListTopicFilterRequest.newBuilder().build(),
                Collections.emptyMap(), RouterServiceGrpc.getListTopiFiltersMethod());
    }
}
