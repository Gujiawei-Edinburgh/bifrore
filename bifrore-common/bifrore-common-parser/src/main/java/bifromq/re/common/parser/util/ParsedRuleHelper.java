package bifromq.re.common.parser.util;

import bifromq.re.common.parser.AliasExpression;
import bifromq.re.common.parser.ParsedRule;
import bifromq.re.common.parser.exception.TopicFilterMissingException;
import bifromq.re.common.parser.exception.UnsupportedSyntaxException;
import io.trino.sql.parser.ParsingOptions;
import io.trino.sql.parser.SqlParser;
import io.trino.sql.tree.*;
import org.mvel2.MVEL;

import java.io.Serializable;
import java.util.HashMap;
import java.util.Map;


public class ParsedRuleHelper {
    private static final SqlParser sqlParser = new SqlParser();

    public static ParsedRule getInstance(String sqlRuleExpression)
            throws TopicFilterMissingException, UnsupportedSyntaxException {
        Query query = (Query) sqlParser.createStatement(sqlRuleExpression, new ParsingOptions());
        QuerySpecification querySpec = (QuerySpecification) query.getQueryBody();
        Serializable compiledCondition = null;
        Map<String, AliasExpression> compiledAliasExpressions = new HashMap<>();
        if (querySpec.getFrom().isEmpty()) {
            throw new TopicFilterMissingException();
        }
        Relation fromClause = querySpec.getFrom().get();
        String topicFilter;
        if (fromClause instanceof Table table) {
            topicFilter = table.getName().toString();
        } else {
            throw new UnsupportedSyntaxException(fromClause.toString());
        }
        if (querySpec.getWhere().isPresent()) {
            Expression whereClause = querySpec.getWhere().get();
            String conditionStr = ExpressionFormatter.formatExpression(whereClause);
            compiledCondition = MVEL.compileExpression(conditionStr);
        }
        for (SelectItem selectItem : querySpec.getSelect().getSelectItems()) {
            if (selectItem instanceof SingleColumn column) {
                String alias = column.toString();
                if (column.getAlias().isPresent()) {
                    alias = column.getAlias().get().toString();
                }
                String expressionStr = ExpressionFormatter.formatExpression(column.getExpression());
                Serializable compiledExpression = MVEL.compileExpression(expressionStr);
                compiledAliasExpressions.put(column.toString(), new AliasExpression(alias, compiledExpression));
            } else if (selectItem instanceof AllColumns columns) {
                compiledAliasExpressions.put(columns.toString(), new AliasExpression(columns.toString(), null));
            }
        }
        return new ParsedRule(compiledCondition, compiledAliasExpressions, topicFilter);
    }
}
