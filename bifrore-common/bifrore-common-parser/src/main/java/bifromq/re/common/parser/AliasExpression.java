package bifromq.re.common.parser;

import java.io.Serial;
import java.io.Serializable;

public class AliasExpression implements Serializable {
    @Serial
    private static final long serialVersionUID = 1L;
    private final String alias;
    private final Serializable compiledExpression;


    public AliasExpression(String alias, Serializable compiledExpression) {
        this.alias = alias;
        this.compiledExpression = compiledExpression;
    }

    public String getAlias() {
        return alias;
    }

    public Serializable getCompiledExpression() {
        return compiledExpression;
    }
}
