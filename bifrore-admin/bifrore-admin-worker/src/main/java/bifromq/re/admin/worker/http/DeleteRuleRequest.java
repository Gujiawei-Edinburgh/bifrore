package bifromq.re.admin.worker.http;

import lombok.Data;

import java.util.List;

@Data
public class DeleteRuleRequest {
    private List<String> ruleIds;
}
