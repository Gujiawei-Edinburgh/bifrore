package bifrore.admin.worker.http;

import java.util.List;

public record AddRuleHttpRequest(String expression, List<String> destinations) {

}
