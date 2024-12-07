package bifrore.common.parser;

import bifrore.commontype.Message;
import com.google.gson.Gson;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.google.protobuf.ByteString;
import org.mvel2.MVEL;

import java.util.HashMap;
import java.util.Map;
import java.util.Optional;

public class RuleEvaluator {
    private final Gson gson = new Gson();

    public Optional<Message> evaluate(Parsed parsed, Message message) {
        Map<String, Object> contextMap = createContextFromPayload(message);
        if (!evaluateCondition(parsed, contextMap)) {
            return Optional.empty();
        }
        return Optional.of(evaluateExpression(parsed, message, contextMap));
    }

    private boolean evaluateCondition(Parsed parsed, Map<String, Object> contextMap) {
        if (parsed.getCompiledCondition() == null) {
            return true;
        }
        return (Boolean) MVEL.executeExpression(parsed.getCompiledCondition(), contextMap);
    }

    private Message evaluateExpression(Parsed parsed, Message message, Map<String, Object> contextMap) {
        Message.Builder builder = Message.newBuilder();
        builder.setQos(message.getQos());
        builder.setTopic(message.getTopic());
        if (parsed.getCompiledAliasExpressions().containsKey("*")) {
            builder.setPayload(message.getPayload());
        }else {
            Map<String, Object> mappedFields = new HashMap<>();
            for (Map.Entry<String, AliasExpression> entry : parsed.getCompiledAliasExpressions().entrySet()) {
                Object newValue = MVEL.executeExpression(entry.getValue().getCompiledExpression(), contextMap);
                mappedFields.put(entry.getValue().getAlias(), newValue);
            }
            byte[] jsonBytes = gson.toJson(mappedFields).getBytes();
            builder.setPayload(ByteString.copyFrom(jsonBytes));
        }
        return builder.build();
    }

    private Map<String, Object> createContextFromPayload(Message message) {
        Map<String, Object> context = new HashMap<>();
        JsonObject payload = parsePayload(message);
        payload.entrySet().forEach(entry -> {
            if (entry.getValue().isJsonPrimitive()) {
                if (entry.getValue().getAsJsonPrimitive().isNumber()) {
                    context.put(entry.getKey(), entry.getValue().getAsNumber().doubleValue());
                } else if (entry.getValue().getAsJsonPrimitive().isBoolean()) {
                    context.put(entry.getKey(), entry.getValue().getAsBoolean());
                } else if (entry.getValue().getAsJsonPrimitive().isString()) {
                    context.put(entry.getKey(), entry.getValue().getAsString());
                }
            }
        });
        return context;
    }

    private JsonObject parsePayload(Message message) {
        String payloadStr = message.getPayload().toStringUtf8();
        return JsonParser.parseString(payloadStr).getAsJsonObject();
    }
}
