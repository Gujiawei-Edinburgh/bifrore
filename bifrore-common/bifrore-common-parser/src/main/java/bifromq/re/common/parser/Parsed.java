package bifromq.re.common.parser;

import java.io.Serial;
import java.io.Serializable;
import java.util.Map;

public class Parsed implements Serializable {
    @Serial
    private static final long serialVersionUID = 1L;
    private final Serializable compiledCondition;
    private final Map<String, AliasExpression> compiledAliasExpressions;

    public Parsed(Serializable compiledCondition, Map<String, AliasExpression> compiledAliasExpressions) {
        this.compiledCondition = compiledCondition;
        this.compiledAliasExpressions = compiledAliasExpressions;
    }

    public Serializable getCompiledCondition() {
        return compiledCondition;
    }

    public Map<String, AliasExpression> getCompiledAliasExpressions() {
        return compiledAliasExpressions;
    }
}
