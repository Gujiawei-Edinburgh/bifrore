package bifrore.admin.worker.http;

import bifrore.router.rpc.proto.RuleMeta;

import java.util.List;

public record ListRuleHttpResponse(List<RuleMeta> ruleMetaList) {
}
