package bifrore.common.parser;

import java.io.Serializable;
import java.util.Map;

public class ParsedRule {
    private final Parsed parsed;
    private final String topicFilter;

    public ParsedRule(Serializable compiledCondition,
                      Map<String, AliasExpression> compiledAliasExpressions,
                      String topicFilter) {
        this.parsed = new Parsed(compiledCondition, compiledAliasExpressions);
        this.topicFilter = topicFilter;
    }

    public Parsed getParsed() {
        return parsed;
    }

    public String getTopicFilter() {
        return topicFilter;
    }
}
