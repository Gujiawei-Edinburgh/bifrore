package com.tenon;

import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.Objects;
import java.util.function.ToDoubleFunction;

final class TenonMetricsBinder {
    private static final String METER_REGISTRY_CLASS = "io.micrometer.core.instrument.MeterRegistry";
    private static final String GAUGE_CLASS = "io.micrometer.core.instrument.Gauge";
    private static final String FUNCTION_COUNTER_CLASS = "io.micrometer.core.instrument.FunctionCounter";

    private TenonMetricsBinder() {}

    static void bind(Tenon engine, Object registry) {
        bind(engine, registry, "tenon");
    }

    static void bind(Tenon engine, Object registry, String prefix) {
        Objects.requireNonNull(engine, "engine");
        Objects.requireNonNull(registry, "registry");
        String metricPrefix = normalizePrefix(prefix);
        TenonMetricsView metrics = new TenonMetricsView(engine);

        bindCounter(registry, metricPrefix + "_evals", metrics, TenonMetricsView::evalCount);
        bindCounter(registry, metricPrefix + "_eval_errors", metrics, TenonMetricsView::evalErrorCount);
        bindCounter(registry, metricPrefix + "_eval_type_errors", metrics, TenonMetricsView::evalTypeErrorCount);
        bindCounter(registry, metricPrefix + "_payload_schema_errors", metrics, TenonMetricsView::payloadSchemaErrorCount);
        bindCounter(registry, metricPrefix + "_payload_decode_errors", metrics, TenonMetricsView::payloadDecodeErrorCount);
        bindCounter(registry, metricPrefix + "_payload_build_errors", metrics, TenonMetricsView::payloadBuildErrorCount);
        bindCounter(registry, metricPrefix + "_core_eval_latency_samples", metrics, TenonMetricsView::coreEvalCount);
        bindCounter(registry, metricPrefix + "_core_eval_latency_nanos", metrics, TenonMetricsView::coreEvalTotalNanos);
        bindGauge(registry, metricPrefix + "_core_eval_latency_max_nanos", metrics, TenonMetricsView::coreEvalMaxNanos);
        bindCounter(registry, metricPrefix + "_worker_pipeline_latency_samples", metrics, TenonMetricsView::workerPipelineCount);
        bindCounter(registry, metricPrefix + "_worker_pipeline_latency_nanos", metrics, TenonMetricsView::workerPipelineTotalNanos);
        bindGauge(registry, metricPrefix + "_worker_pipeline_latency_max_nanos", metrics, TenonMetricsView::workerPipelineMaxNanos);
        bindCounter(registry, metricPrefix + "_topic_match_latency_samples", metrics, TenonMetricsView::topicMatchCount);
        bindCounter(registry, metricPrefix + "_topic_match_latency_nanos", metrics, TenonMetricsView::topicMatchTotalNanos);
        bindGauge(registry, metricPrefix + "_topic_match_latency_max_nanos", metrics, TenonMetricsView::topicMatchMaxNanos);
        bindCounter(registry, metricPrefix + "_payload_decode_latency_samples", metrics, TenonMetricsView::payloadDecodeCount);
        bindCounter(registry, metricPrefix + "_payload_decode_latency_nanos", metrics, TenonMetricsView::payloadDecodeTotalNanos);
        bindGauge(registry, metricPrefix + "_payload_decode_latency_max_nanos", metrics, TenonMetricsView::payloadDecodeMaxNanos);
        bindCounter(registry, metricPrefix + "_msg_ir_build_latency_samples", metrics, TenonMetricsView::msgIrBuildCount);
        bindCounter(registry, metricPrefix + "_msg_ir_build_latency_nanos", metrics, TenonMetricsView::msgIrBuildTotalNanos);
        bindGauge(registry, metricPrefix + "_msg_ir_build_latency_max_nanos", metrics, TenonMetricsView::msgIrBuildMaxNanos);
        bindCounter(registry, metricPrefix + "_predicate_latency_samples", metrics, TenonMetricsView::predicateCount);
        bindCounter(registry, metricPrefix + "_predicate_latency_nanos", metrics, TenonMetricsView::predicateTotalNanos);
        bindGauge(registry, metricPrefix + "_predicate_latency_max_nanos", metrics, TenonMetricsView::predicateMaxNanos);
        bindCounter(registry, metricPrefix + "_projection_latency_samples", metrics, TenonMetricsView::projectionCount);
        bindCounter(registry, metricPrefix + "_projection_latency_nanos", metrics, TenonMetricsView::projectionTotalNanos);
        bindGauge(registry, metricPrefix + "_projection_latency_max_nanos", metrics, TenonMetricsView::projectionMaxNanos);
        bindCounter(registry, metricPrefix + "_ingress_messages", metrics, TenonMetricsView::ingressMessageCount);
        bindGauge(registry, metricPrefix + "_core_queue_depth", metrics, TenonMetricsView::coreQueueDepth);
        bindGauge(registry, metricPrefix + "_core_queue_depth_max", metrics, TenonMetricsView::coreQueueDepthMax);
        bindCounter(registry, metricPrefix + "_core_queue_drops", metrics, TenonMetricsView::coreQueueDropCount);
        bindGauge(registry, metricPrefix + "_ffi_queue_depth", metrics, TenonMetricsView::ffiQueueDepth);
        bindGauge(registry, metricPrefix + "_ffi_queue_depth_max", metrics, TenonMetricsView::ffiQueueDepthMax);
        bindCounter(registry, metricPrefix + "_ffi_queue_drops", metrics, TenonMetricsView::ffiQueueDropCount);
        bindCounter(registry, metricPrefix + "_callback_drops", metrics, TenonMetricsView::callbackDroppedCount);
        bindGauge(registry, metricPrefix + "_callback_pending_count", metrics, TenonMetricsView::callbackPendingCount);
        bindGauge(registry, metricPrefix + "_callback_queue_depth", metrics, TenonMetricsView::callbackQueueDepth);
        bindCounter(registry, metricPrefix + "_heap_poll_errors", metrics, TenonMetricsView::heapPollErrorCount);
        bindCounter(registry, metricPrefix + "_heap_poll_invalid_arguments", metrics, TenonMetricsView::heapPollInvalidArgumentCount);
        bindCounter(registry, metricPrefix + "_heap_poll_invalid_states", metrics, TenonMetricsView::heapPollInvalidStateCount);
        bindCounter(registry, metricPrefix + "_heap_poll_internal_queue_errors", metrics, TenonMetricsView::heapPollInternalQueueErrorCount);
        bindCounter(registry, metricPrefix + "_heap_poll_unknown_errors", metrics, TenonMetricsView::heapPollUnknownErrorCount);
        bindCounter(registry, metricPrefix + "_heap_poll_messages", metrics, TenonMetricsView::heapPollMessageCount);
        bindCounter(registry, metricPrefix + "_heap_poll_payload_bytes", metrics, TenonMetricsView::heapPollPayloadBytes);
        bindCounter(registry, metricPrefix + "_heap_poll_empty_polls", metrics, TenonMetricsView::heapPollNoDataCount);
        bindCounter(registry, metricPrefix + "_shutdown_drops", metrics, TenonMetricsView::shutdownDroppedCount);
        bindCounter(registry, metricPrefix + "_poller_timeout_pending_events", metrics, TenonMetricsView::pollerTimeoutPendingCount);
    }

    private static String normalizePrefix(String prefix) {
        if (prefix == null || prefix.isBlank()) {
            return "tenon";
        }
        return prefix.trim();
    }

    private static void bindGauge(
        Object registry,
        String name,
        TenonMetricsView metrics,
        ToDoubleFunction<TenonMetricsView> extractor
    ) {
        bindMeter(registry, GAUGE_CLASS, name, metrics, extractor);
    }

    private static void bindCounter(
        Object registry,
        String name,
        TenonMetricsView metrics,
        ToDoubleFunction<TenonMetricsView> extractor
    ) {
        bindMeter(registry, FUNCTION_COUNTER_CLASS, name, metrics, extractor);
    }

    private static void bindMeter(
        Object registry,
        String meterClassName,
        String name,
        TenonMetricsView metrics,
        ToDoubleFunction<TenonMetricsView> extractor
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
            throw new IllegalStateException("Failed to bind Tenon metrics", cause);
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
