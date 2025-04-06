package bifrore.common.parser.util;


import io.trino.sql.tree.ArithmeticBinaryExpression;
import io.trino.sql.tree.Cast;
import io.trino.sql.tree.ComparisonExpression;
import io.trino.sql.tree.DereferenceExpression;
import io.trino.sql.tree.DoubleLiteral;
import io.trino.sql.tree.Expression;
import io.trino.sql.tree.Identifier;
import io.trino.sql.tree.Literal;
import io.trino.sql.tree.LongLiteral;
import io.trino.sql.tree.StringLiteral;

public class ExpressionFormatter {

    public static String formatExpression(Expression expression) {
        if (expression instanceof Identifier) {
            return ((Identifier) expression).getValue();
        } else if (expression instanceof Literal) {
            return formatLiteral((Literal) expression);
        } else if (expression instanceof ArithmeticBinaryExpression) {
            return formatArithmeticBinaryExpression((ArithmeticBinaryExpression) expression);
        } else if (expression instanceof ComparisonExpression) {
            return formatComparisonExpression((ComparisonExpression) expression);
        } else if (expression instanceof DereferenceExpression) {
            return formatDereferenceExpression((DereferenceExpression) expression);
        } else if (expression instanceof Cast) {
            return formatCastExpression((Cast) expression);
        }
        throw new UnsupportedOperationException("Unsupported expression: " + expression.getClass());
    }

    private static String formatLiteral(Literal literal) {
        if (literal instanceof StringLiteral) {
            return "'" + ((StringLiteral) literal).getValue() + "'";
        } else if (literal instanceof LongLiteral) {
            return String.valueOf(((LongLiteral) literal).getValue());
        } else if (literal instanceof DoubleLiteral) {
            return String.valueOf(((DoubleLiteral) literal).getValue());
        }
        throw new UnsupportedOperationException("Unsupported literal: " + literal.getClass());
    }

    private static String formatArithmeticBinaryExpression(ArithmeticBinaryExpression expression) {
        String left = formatExpression(expression.getLeft());
        String right = formatExpression(expression.getRight());
        String operator = expression.getOperator().getValue();
        return "(" + left + " " + operator + " " + right + ")";
    }

    private static String formatComparisonExpression(ComparisonExpression expression) {
        String left = formatExpression(expression.getLeft());
        String right = formatExpression(expression.getRight());
        String operator = expression.getOperator().getValue();
        return "(" + left + " " + operator + " " + right + ")";
    }

    private static String formatDereferenceExpression(DereferenceExpression expression) {
        return formatExpression(expression.getBase()) + "." + expression.getField().get().getValue();
    }

    private static String formatCastExpression(Cast expression) {
        return formatExpression(expression.getExpression()) + "::" + expression.getType().toString();
    }

//    private static String formatLogicalBinaryExpression(LogicalBinaryExpression expression) {
//        String operator = expression.getOperator().toString();
//        String left = formatExpression(expression.getLeft());
//        String right = formatExpression(expression.getRight());
//        return "(" + left + " " + operator + " " + right + ")";
//    }
}

