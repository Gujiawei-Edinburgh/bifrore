package bifrore.common.parser;

import bifrore.common.parser.exception.TopicFilterMissingException;
import bifrore.common.parser.exception.UnsupportedSyntaxException;
import bifrore.commontype.Message;
import bifrore.commontype.QoS;
import bifrore.common.parser.util.ParsedRuleHelper;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.google.protobuf.ByteString;
import org.testng.annotations.Test;

import java.nio.charset.StandardCharsets;
import java.util.Optional;

import static org.testng.AssertJUnit.fail;

public class RuleEvaluatorTest {
    private final Message inputMessage = Message.newBuilder()
            .setQos(QoS.AT_LEAST_ONCE)
            .setTopic("testTopic")
            .setPayload(ByteString.copyFrom("{\"height\": 5, \"pressure\": 10, \"temp\": 31}".getBytes()))
            .build();
    private final RuleEvaluator evaluator = new RuleEvaluator();

    private JsonObject getJsonObjectFromMessage(Message message) {
        byte[] payload = message.getPayload().toByteArray();
        String jsonString = new String(payload, StandardCharsets.UTF_8);
        return JsonParser.parseString(jsonString).getAsJsonObject();
    }

    @Test
    public void testSelectAllFieldsWithCondition() {
        String sql = "SELECT * FROM data where temp > 30";
        try {
            ParsedRule parsedRule = ParsedRuleHelper.getInstance(sql);
            assert "data".equals(parsedRule.getTopicFilter());
            Optional<Message> message = evaluator.evaluate(parsedRule.getParsed(), inputMessage);
            assert message.isPresent();
            JsonObject jsonObject = getJsonObjectFromMessage(message.get());
            assert jsonObject.get("height").getAsDouble() == 5.0;
            assert jsonObject.get("pressure").getAsDouble() == 10.0;
            assert jsonObject.get("temp").getAsDouble() == 31.0;
        }catch (TopicFilterMissingException | UnsupportedSyntaxException ex) {
            fail();
        }
    }

    @Test
    public void testSelectAllFieldsWithoutCondition() {
        String sql = "select * from data";
        try {
            ParsedRule parsedRule = ParsedRuleHelper.getInstance(sql);
            assert "data".equals(parsedRule.getTopicFilter());
            Optional<Message> message = evaluator.evaluate(parsedRule.getParsed(), inputMessage);
            assert message.isPresent();
            JsonObject jsonObject = getJsonObjectFromMessage(message.get());
            assert jsonObject.get("height").getAsDouble() == 5.0;
            assert jsonObject.get("pressure").getAsDouble() == 10.0;
            assert jsonObject.get("temp").getAsDouble() == 31.0;
        }catch (TopicFilterMissingException | UnsupportedSyntaxException ex) {
            fail();
        }
    }

    @Test
    public void testSelectOneFiledWithCondition() {
        String sql = "select height as h from data where temp > 30";
        try {
            ParsedRule parsedRule = ParsedRuleHelper.getInstance(sql);
            Optional<Message> message = evaluator.evaluate(parsedRule.getParsed(), inputMessage);
            assert message.isPresent();
            JsonObject jsonObject = getJsonObjectFromMessage(message.get());
            assert jsonObject.size() == 1;
            assert jsonObject.get("h").getAsDouble() == 5.0;
        }catch (TopicFilterMissingException | UnsupportedSyntaxException ex) {
            fail();
        }
    }

    @Test
    public void testSelectTwoFiledWithCondition() {
        String sql = "select height as h, pressure from data where temp > 30";
        try {
            ParsedRule parsedRule = ParsedRuleHelper.getInstance(sql);
            Optional<Message> message = evaluator.evaluate(parsedRule.getParsed(), inputMessage);
            assert message.isPresent();
            JsonObject jsonObject = getJsonObjectFromMessage(message.get());
            assert jsonObject.size() == 2;
            assert jsonObject.get("h").getAsDouble() == 5.0;
            assert jsonObject.get("pressure").getAsDouble() == 10.0;
        }catch (TopicFilterMissingException | UnsupportedSyntaxException ex) {
            fail();
        }
    }

    @Test
    public void testSelectTwoFiledMappedWithCondition() {
        String sql = "select (height + 2) * 2 as h, pressure / 2 as p from data where temp > 30";
        try {
            ParsedRule parsedRule = ParsedRuleHelper.getInstance(sql);
            Optional<Message> message = evaluator.evaluate(parsedRule.getParsed(), inputMessage);
            assert message.isPresent();
            JsonObject jsonObject = getJsonObjectFromMessage(message.get());
            assert jsonObject.size() == 2;
            assert jsonObject.get("h").getAsDouble() == 14.0;
            assert jsonObject.get("p").getAsDouble() == 5.0;
        }catch (TopicFilterMissingException | UnsupportedSyntaxException ex) {
            fail();;
        }
    }

    @Test
    public void testFiltering() {
        String sql = "select (height + 2) * 2 as h, pressure / 2 as p from data where temp > 40";
        try {
            ParsedRule parsedRule = ParsedRuleHelper.getInstance(sql);
            Optional<Message> message = evaluator.evaluate(parsedRule.getParsed(), inputMessage);
            assert message.isEmpty();
        }catch (TopicFilterMissingException | UnsupportedSyntaxException ex) {
            fail();
        }
    }

    @Test
    public void testMultipleTopicFilters() {
        String sql = "select * from d1, d2, d3 where temp > 40";
        try {
            ParsedRuleHelper.getInstance(sql);
        }catch (TopicFilterMissingException | UnsupportedSyntaxException ex) {
            assert ex instanceof UnsupportedSyntaxException;
        }
    }

    @Test
    public void testTopicFilterMissing() {
        String sql = "select * where temp > 40";
        try {
            ParsedRuleHelper.getInstance(sql);
        }catch (TopicFilterMissingException | UnsupportedSyntaxException ex) {
            assert ex instanceof TopicFilterMissingException;
        }
    }

    @Test
    public void testUnsupportedSyntax() {
        String sql = "select * from (select p from d1) where temp > 40";
        try {
            ParsedRuleHelper.getInstance(sql);
        }catch (TopicFilterMissingException | UnsupportedSyntaxException ex) {
            assert ex instanceof UnsupportedSyntaxException;
        }
    }

    @Test
    public void testTopicFilter() {
        String sql = "select * from \"a/b/c\" where temp > 40";
        try {
            ParsedRule parsedRule = ParsedRuleHelper.getInstance(sql);
            assert "a/b/c".equals(parsedRule.getTopicFilter());
        }catch (TopicFilterMissingException | UnsupportedSyntaxException ex) {
            fail();
        }
    }
}
