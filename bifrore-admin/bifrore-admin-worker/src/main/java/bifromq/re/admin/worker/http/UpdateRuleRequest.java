package bifromq.re.admin.worker.http;

import lombok.Data;

@Data
public class UpdateRuleRequest {
    private String ruleId;
    private String topicFilter;
    private String expression;
}
