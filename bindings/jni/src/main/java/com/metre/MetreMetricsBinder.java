package com.metre;

import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.Objects;
import java.util.function.ToDoubleFunction;

final class MetreMetricsBinder {
    private static final String METER_REGISTRY_CLASS = "io.micrometer.core.instrument.MeterRegistry";
    private static final String GAUGE_CLASS = "io.micrometer.core.instrument.Gauge";
    private static final String FUNCTION_COUNTER_CLASS = "io.micrometer.core.instrument.FunctionCounter";

    private MetreMetricsBinder() {}

    static void bind(Metre engine, Object registry) {
        bind(engine, registry, "metre");
    }

    static void bind(Metre engine, Object registry, String prefix) {
        Objects.requireNonNull(engine, "engine");
        Objects.requireNonNull(registry, "registry");
        String metricPrefix = normalizePrefix(prefix);
        MetreMetricsView metrics = new MetreMetricsView(engine);

        bindCounter(registry, metricPrefix + "_evals", metrics, MetreMetricsView::evalCount);
        bindCounter(registry, metricPrefix + "_eval_errors", metrics, MetreMetricsView::evalErrorCount);
        bindCounter(registry, metricPrefix + "_eval_type_errors", metrics, MetreMetricsView::evalTypeErrorCount);
        bindCounter(registry, metricPrefix + "_payload_schema_errors", metrics, MetreMetricsView::payloadSchemaErrorCount);
        bindCounter(registry, metricPrefix + "_payload_decode_errors", metrics, MetreMetricsView::payloadDecodeErrorCount);
        bindCounter(registry, metricPrefix + "_payload_build_errors", metrics, MetreMetricsView::payloadBuildErrorCount);
        bindCounter(registry, metricPrefix + "_core_eval_latency_samples", metrics, MetreMetricsView::coreEvalCount);
        bindCounter(registry, metricPrefix + "_core_eval_latency_nanos", metrics, MetreMetricsView::coreEvalTotalNanos);
        bindGauge(registry, metricPrefix + "_core_eval_latency_max_nanos", metrics, MetreMetricsView::coreEvalMaxNanos);
        bindCounter(registry, metricPrefix + "_worker_pipeline_latency_samples", metrics, MetreMetricsView::workerPipelineCount);
        bindCounter(registry, metricPrefix + "_worker_pipeline_latency_nanos", metrics, MetreMetricsView::workerPipelineTotalNanos);
        bindGauge(registry, metricPrefix + "_worker_pipeline_latency_max_nanos", metrics, MetreMetricsView::workerPipelineMaxNanos);
        bindCounter(registry, metricPrefix + "_topic_match_latency_samples", metrics, MetreMetricsView::topicMatchCount);
        bindCounter(registry, metricPrefix + "_topic_match_latency_nanos", metrics, MetreMetricsView::topicMatchTotalNanos);
        bindGauge(registry, metricPrefix + "_topic_match_latency_max_nanos", metrics, MetreMetricsView::topicMatchMaxNanos);
        bindCounter(registry, metricPrefix + "_payload_decode_latency_samples", metrics, MetreMetricsView::payloadDecodeCount);
        bindCounter(registry, metricPrefix + "_payload_decode_latency_nanos", metrics, MetreMetricsView::payloadDecodeTotalNanos);
        bindGauge(registry, metricPrefix + "_payload_decode_latency_max_nanos", metrics, MetreMetricsView::payloadDecodeMaxNanos);
        bindCounter(registry, metricPrefix + "_msg_ir_build_latency_samples", metrics, MetreMetricsView::msgIrBuildCount);
        bindCounter(registry, metricPrefix + "_msg_ir_build_latency_nanos", metrics, MetreMetricsView::msgIrBuildTotalNanos);
        bindGauge(registry, metricPrefix + "_msg_ir_build_latency_max_nanos", metrics, MetreMetricsView::msgIrBuildMaxNanos);
        bindCounter(registry, metricPrefix + "_predicate_latency_samples", metrics, MetreMetricsView::predicateCount);
        bindCounter(registry, metricPrefix + "_predicate_latency_nanos", metrics, MetreMetricsView::predicateTotalNanos);
        bindGauge(registry, metricPrefix + "_predicate_latency_max_nanos", metrics, MetreMetricsView::predicateMaxNanos);
        bindCounter(registry, metricPrefix + "_projection_latency_samples", metrics, MetreMetricsView::projectionCount);
        bindCounter(registry, metricPrefix + "_projection_latency_nanos", metrics, MetreMetricsView::projectionTotalNanos);
        bindGauge(registry, metricPrefix + "_projection_latency_max_nanos", metrics, MetreMetricsView::projectionMaxNanos);
        bindCounter(registry, metricPrefix + "_ingress_messages", metrics, MetreMetricsView::ingressMessageCount);
        bindGauge(registry, metricPrefix + "_core_queue_depth", metrics, MetreMetricsView::coreQueueDepth);
        bindGauge(registry, metricPrefix + "_core_queue_depth_max", metrics, MetreMetricsView::coreQueueDepthMax);
        bindCounter(registry, metricPrefix + "_core_queue_drops", metrics, MetreMetricsView::coreQueueDropCount);
        bindGauge(registry, metricPrefix + "_ffi_queue_depth", metrics, MetreMetricsView::ffiQueueDepth);
        bindGauge(registry, metricPrefix + "_ffi_queue_depth_max", metrics, MetreMetricsView::ffiQueueDepthMax);
        bindCounter(registry, metricPrefix + "_ffi_queue_drops", metrics, MetreMetricsView::ffiQueueDropCount);
        bindCounter(registry, metricPrefix + "_callback_drops", metrics, MetreMetricsView::callbackDroppedCount);
        bindGauge(registry, metricPrefix + "_callback_pending_count", metrics, MetreMetricsView::callbackPendingCount);
        bindGauge(registry, metricPrefix + "_callback_queue_depth", metrics, MetreMetricsView::callbackQueueDepth);
        bindCounter(registry, metricPrefix + "_heap_poll_errors", metrics, MetreMetricsView::heapPollErrorCount);
        bindCounter(registry, metricPrefix + "_heap_poll_invalid_arguments", metrics, MetreMetricsView::heapPollInvalidArgumentCount);
        bindCounter(registry, metricPrefix + "_heap_poll_invalid_states", metrics, MetreMetricsView::heapPollInvalidStateCount);
        bindCounter(registry, metricPrefix + "_heap_poll_internal_queue_errors", metrics, MetreMetricsView::heapPollInternalQueueErrorCount);
        bindCounter(registry, metricPrefix + "_heap_poll_unknown_errors", metrics, MetreMetricsView::heapPollUnknownErrorCount);
        bindCounter(registry, metricPrefix + "_heap_poll_messages", metrics, MetreMetricsView::heapPollMessageCount);
        bindCounter(registry, metricPrefix + "_heap_poll_payload_bytes", metrics, MetreMetricsView::heapPollPayloadBytes);
        bindCounter(registry, metricPrefix + "_heap_poll_empty_polls", metrics, MetreMetricsView::heapPollNoDataCount);
        bindCounter(registry, metricPrefix + "_shutdown_drops", metrics, MetreMetricsView::shutdownDroppedCount);
        bindCounter(registry, metricPrefix + "_poller_timeout_pending_events", metrics, MetreMetricsView::pollerTimeoutPendingCount);
    }

    private static String normalizePrefix(String prefix) {
        if (prefix == null || prefix.isBlank()) {
            return "metre";
        }
        return prefix.trim();
    }

    private static void bindGauge(
        Object registry,
        String name,
        MetreMetricsView metrics,
        ToDoubleFunction<MetreMetricsView> extractor
    ) {
        bindMeter(registry, GAUGE_CLASS, name, metrics, extractor);
    }

    private static void bindCounter(
        Object registry,
        String name,
        MetreMetricsView metrics,
        ToDoubleFunction<MetreMetricsView> extractor
    ) {
        bindMeter(registry, FUNCTION_COUNTER_CLASS, name, metrics, extractor);
    }

    private static void bindMeter(
        Object registry,
        String meterClassName,
        String name,
        MetreMetricsView metrics,
        ToDoubleFunction<MetreMetricsView> extractor
    ) {
        try {
            Class<?> meterRegistryClass = Class.forName(METER_REGISTRY_CLASS);
            if (!meterRegistryClass.isInstance(registry)) {
                throw new IllegalArgumentException(
                    "registry must be an instance of " + METER_REGISTRY_CLASS
                );
            }
            Class<?> meterClass = Class.forName(meterClassName);
            Method builderMethod =
                meterClass.getMethod("builder", String.class, Object.class, ToDoubleFunction.class);
            Object builder = builderMethod.invoke(null, name, metrics, extractor);
            useStrongReference(builder);
            Method registerMethod = builder.getClass().getMethod("register", meterRegistryClass);
            registerMethod.invoke(builder, registry);
        } catch (ClassNotFoundException e) {
            throw new IllegalStateException(
                "Micrometer is not on the classpath; add io.micrometer:micrometer-core",
                e
            );
        } catch (NoSuchMethodException | IllegalAccessException e) {
            throw new IllegalStateException("Unsupported Micrometer version", e);
        } catch (InvocationTargetException e) {
            Throwable cause = e.getCause();
            if (cause instanceof RuntimeException) {
                throw (RuntimeException) cause;
            }
            throw new IllegalStateException("Failed to bind Metre metrics", cause);
        }
    }

    private static void useStrongReference(Object builder) {
        try {
            Method strongReferenceMethod = builder.getClass().getMethod("strongReference", boolean.class);
            strongReferenceMethod.invoke(builder, true);
        } catch (NoSuchMethodException ignored) {
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("Failed to configure Micrometer strong reference", e);
        }
    }
}
