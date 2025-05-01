package bifrore.common.parser;

import bifrore.commontype.Message;
import bifrore.monitoring.metrics.SysMeter;
import com.google.gson.Gson;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.google.protobuf.ByteString;
import io.micrometer.core.instrument.Timer;
import lombok.extern.slf4j.Slf4j;
import org.mvel2.MVEL;

import java.util.HashMap;
import java.util.Map;
import java.util.Optional;

import static bifrore.monitoring.metrics.SysMetric.EvaluatedLatency;
import static bifrore.monitoring.metrics.SysMetric.MessageParseFailureCount;

@Slf4j
public class RuleEvaluator {
    private final Gson gson = new Gson();

    public Optional<Message> evaluate(Parsed parsed, Message.Builder messageBuilder) {
        Timer.Sample sampler = Timer.start();
        Map<String, Object> contextMap = createContextFromPayload(messageBuilder.getPayload());
        if (contextMap.isEmpty() || !evaluateCondition(parsed, contextMap)) {
            return Optional.empty();
        }

        Optional<Message> evaluated = Optional.of(evaluateExpression(parsed, messageBuilder, contextMap));
        sampler.stop(SysMeter.INSTANCE.timer(EvaluatedLatency));
        return evaluated;
    }

    private boolean evaluateCondition(Parsed parsed, Map<String, Object> contextMap) {
        if (parsed.getCompiledCondition() == null) {
            return true;
        }
        return (Boolean) MVEL.executeExpression(parsed.getCompiledCondition(), contextMap);
    }

    private Message evaluateExpression(Parsed parsed, Message.Builder messageBuilder, Map<String, Object> contextMap) {
        if (!parsed.getCompiledAliasExpressions().containsKey("*")) {
            Map<String, Object> mappedFields = new HashMap<>();
            for (Map.Entry<String, AliasExpression> entry : parsed.getCompiledAliasExpressions().entrySet()) {
                Object newValue = MVEL.executeExpression(entry.getValue().getCompiledExpression(), contextMap);
                mappedFields.put(entry.getValue().getAlias(), newValue);
            }
            byte[] jsonBytes = gson.toJson(mappedFields).getBytes();
            messageBuilder.setPayload(ByteString.copyFrom(jsonBytes));
        }
        return messageBuilder.build();
    }

    private Map<String, Object> createContextFromPayload(ByteString msgPayload) {
        Map<String, Object> context = new HashMap<>();
        try {
            JsonObject payload = JsonParser.parseString(msgPayload.toStringUtf8()).getAsJsonObject();
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
        }catch (Exception e) {
            SysMeter.INSTANCE.recordCount(MessageParseFailureCount);
            log.error("Error parsing payload: {}", msgPayload, e);
        }
        return context;
    }
}
