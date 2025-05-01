package bifrore.router.client;

import bifrore.baserpc.IRPCClient;
import bifrore.common.parser.Parsed;
import bifrore.common.parser.util.ParsedSerializeUtil;

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
                            Parsed parsed = ParsedSerializeUtil.deserializeParsed(compiledRule
                                    .getExpressionObj().toByteArray());
                            return new Matched(parsed, compiledRule.getDestinationsList(), compiledRule.getAliasedTopicFilter());
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
