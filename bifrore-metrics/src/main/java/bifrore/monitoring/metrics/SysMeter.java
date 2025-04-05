package bifrore.monitoring.metrics;

import io.micrometer.core.instrument.Counter;
import io.micrometer.core.instrument.Gauge;
import io.micrometer.core.instrument.Meter;
import io.micrometer.core.instrument.Metrics;
import io.micrometer.core.instrument.Tags;

import java.util.HashMap;
import java.util.Map;
import java.util.function.Supplier;

public class SysMeter {
    public static SysMeter INSTANCE = new SysMeter();
    private final Map<SysMetric, Meter> meters = new HashMap<>();
    private final Map<SysMetric, Gauge> gauges = new HashMap<>();
    private Tags tags = Tags.of("system", "bifrore");;

    SysMeter() {
        for (SysMetric metric : SysMetric.values()) {
            switch (metric.meterType) {
                case COUNTER:
                    meters.put(metric, Metrics.counter(metric.metricName, tags));
                    break;
                case TIMER:
                    meters.put(metric, Metrics.timer(metric.metricName, tags));
                    break;
                case DISTRIBUTION_SUMMARY:
                    meters.put(metric, Metrics.summary(metric.metricName, tags));
                    break;
                case GAUGE:
                    break;
                default:
                    throw new UnsupportedOperationException("Unsupported traffic meter type");
            }
        }
    }

    public void recordCount(SysMetric metric) {
        ((Counter)meters.get(metric)).increment();
    }

    public void startGauge(SysMetric metric, Supplier<Number> valueSupplier) {
        assert metric.meterType == Meter.Type.GAUGE;
        Gauge gauge = Gauge.builder(metric.metricName, valueSupplier)
                .tags(tags)
                .register(Metrics.globalRegistry);
        gauges.put(metric, gauge);
    }

    public void stopGauge(SysMetric metric) {
        Gauge gauge = gauges.remove(metric);
        if (gauge != null) {
            Metrics.globalRegistry.remove(gauge);
        }
    }
}
