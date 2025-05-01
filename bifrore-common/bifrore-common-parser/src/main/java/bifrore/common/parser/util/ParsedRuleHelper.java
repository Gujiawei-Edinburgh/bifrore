package bifrore.common.parser.util;

import bifrore.common.parser.AliasExpression;
import bifrore.common.parser.ParsedRule;
import bifrore.common.parser.exception.TopicFilterMissingException;
import bifrore.common.parser.exception.UnsupportedSyntaxException;
import bifrore.monitoring.metrics.SysMeter;
import io.trino.sql.parser.ParsingOptions;
import io.trino.sql.parser.SqlParser;

import io.trino.sql.tree.AliasedRelation;
import io.trino.sql.tree.AllColumns;
import io.trino.sql.tree.Expression;
import io.trino.sql.tree.Query;
import io.trino.sql.tree.QuerySpecification;
import io.trino.sql.tree.Relation;
import io.trino.sql.tree.SelectItem;
import io.trino.sql.tree.SingleColumn;
import io.trino.sql.tree.Table;
import org.mvel2.MVEL;

import java.io.Serializable;
import java.util.HashMap;
import java.util.Map;

import static bifrore.monitoring.metrics.SysMetric.RuleSyntaxErrorCount;
import static bifrore.monitoring.metrics.SysMetric.RuleTopicFilterMissingCount;


public class ParsedRuleHelper {
    private static final SqlParser sqlParser = new SqlParser();

    public static ParsedRule getInstance(String sqlRuleExpression)
            throws TopicFilterMissingException, UnsupportedSyntaxException {
        Query query = (Query) sqlParser.createStatement(sqlRuleExpression, new ParsingOptions());
        QuerySpecification querySpec = (QuerySpecification) query.getQueryBody();
        Serializable compiledCondition = null;
        Map<String, AliasExpression> compiledAliasExpressions = new HashMap<>();
        if (querySpec.getFrom().isEmpty()) {
            SysMeter.INSTANCE.recordCount(RuleTopicFilterMissingCount);
            throw new TopicFilterMissingException();
        }
        Relation fromClause = querySpec.getFrom().get();
        String topicFilter;
        String aliasedTopicFilter;
        if (fromClause instanceof Table table) {
            topicFilter = table.getName().toString();
            aliasedTopicFilter = topicFilter;
        } else if (fromClause instanceof AliasedRelation aliasedRelation) {
            topicFilter = ((Table) aliasedRelation.getRelation()).getName().toString();
            aliasedTopicFilter = aliasedRelation.getAlias().toString();
        } else {
            SysMeter.INSTANCE.recordCount(RuleSyntaxErrorCount);
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
        return new ParsedRule(compiledCondition, compiledAliasExpressions, topicFilter, aliasedTopicFilter);
    }
}
