package bifromq.re.admin.worker.http;

import bifromq.re.router.rpc.proto.RuleMeta;

import java.util.List;

public record ListRuleHttpResponse(List<RuleMeta> ruleMetaList) {
}
