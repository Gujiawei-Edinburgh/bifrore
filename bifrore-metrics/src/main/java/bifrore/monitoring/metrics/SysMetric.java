package bifrore.monitoring.metrics;

import io.micrometer.core.instrument.Meter;

public enum SysMetric {
    // Admin related
    HttpAddDestinationCount("http.add.destination.count", Meter.Type.COUNTER),
    HttpAddDestinationFailureCount("http.add.destination.failure.count", Meter.Type.COUNTER),
    HttpAddRuleCount("http.add.rule.count", Meter.Type.COUNTER),
    HttpAddRuleFailureCount("http.add.rule.failure.count", Meter.Type.COUNTER),
    // Router related
    RuleNumGauge("rule.num.gauge", Meter.Type.GAUGE),
    // Parsing related
    RuleSyntaxErrorCount("rule.syntax.error.count", Meter.Type.COUNTER),
    RuleTopicFilterMissingCount("rule.topicfilter.missing.count", Meter.Type.COUNTER),
    MessageParseFailureCount("message.parse.failure.count", Meter.Type.COUNTER),
    // Producer Related
    ProducerMissCount("producer.miss.count", Meter.Type.COUNTER),
    DestinationMissCount("destination.miss.count", Meter.Type.COUNTER),
    DestinationNumGauge("destination.num.gauge", Meter.Type.GAUGE);

    public final String metricName;
    public final Meter.Type meterType;

    SysMetric(String metricName, Meter.Type meterType) {
        this.metricName = metricName;
        this.meterType = meterType;
    }
}
