package bifrore.common.parser;

import java.io.Serializable;
import java.util.Map;

public class ParsedRule {
    private final Parsed parsed;
    private final String topicFilter;
    private final String aliasedTopicFilter;

    public ParsedRule(Serializable compiledCondition,
                      Map<String, AliasExpression> compiledAliasExpressions,
                      String topicFilter,
                      String aliasedTopicFilter) {
        this.parsed = new Parsed(compiledCondition, compiledAliasExpressions);
        this.topicFilter = topicFilter;
        this.aliasedTopicFilter = aliasedTopicFilter;
    }

    public Parsed getParsed() {
        return parsed;
    }

    public String getTopicFilter() {
        return topicFilter;
    }

    public String getAliasedTopicFilter() {
        return aliasedTopicFilter;
    }
}
