package bifromq.re.starter;

import bifromq.re.starter.config.StandaloneConfig;
import bifromq.re.starter.config.model.SSLContextConfig;
import bifromq.re.starter.config.model.ServerSSLContextConfig;
import bifromq.re.starter.utils.ConfigUtil;
import bifromq.re.starter.utils.ResourceUtil;
import io.micrometer.core.instrument.Metrics;
import io.micrometer.core.instrument.binder.jvm.*;
import io.micrometer.core.instrument.binder.netty4.NettyAllocatorMetrics;
import io.micrometer.core.instrument.binder.system.ProcessorMetrics;
import io.micrometer.core.instrument.binder.system.UptimeMetrics;
import io.netty.buffer.PooledByteBufAllocator;
import io.netty.buffer.UnpooledByteBufAllocator;
import io.netty.handler.ssl.*;
import io.netty.handler.ssl.util.InsecureTrustManagerFactory;
import io.reactivex.rxjava3.plugins.RxJavaPlugins;
import lombok.extern.slf4j.Slf4j;

import java.io.File;
import java.security.Provider;
import java.security.Security;
import java.util.LinkedList;
import java.util.List;

@Slf4j
public abstract class BaseStarter implements IStarter {
    static {
        RxJavaPlugins.setErrorHandler(e -> log.error("Uncaught RxJava exception", e));
    }

    private static File loadFromConfDir(String fileName) {
        return ResourceUtil.getFile(fileName, CONF_DIR_PROP);
    }

    public static final String CONF_DIR_PROP = "CONF_DIR";
    private final List<AutoCloseable> closeableList = new LinkedList<>();

    protected abstract void init(StandaloneConfig config);

    protected abstract Class<StandaloneConfig> configClass();

    protected StandaloneConfig buildConfig(File configFile) {
        return ConfigUtil.build(configFile, configClass());
    }

    protected SslContext buildServerSslContext(ServerSSLContextConfig config) {
        try {
            SslProvider sslProvider = defaultSslProvider();
            SslContextBuilder sslCtxBuilder = SslContextBuilder
                .forServer(loadFromConfDir(config.getCertFile()), loadFromConfDir(config.getKeyFile()))
                .clientAuth(ClientAuth.valueOf(config.getClientAuth()))
                .sslProvider(sslProvider);
            if (config.getTrustCertsFile() == null || config.getTrustCertsFile().isEmpty()) {
                sslCtxBuilder.trustManager(InsecureTrustManagerFactory.INSTANCE);
            } else {
                sslCtxBuilder.trustManager(loadFromConfDir(config.getTrustCertsFile()));
            }
            if (sslProvider == SslProvider.JDK) {
                sslCtxBuilder.sslContextProvider(findJdkProvider());
            }
            return sslCtxBuilder.build();
        } catch (Throwable e) {
            throw new RuntimeException("Fail to initialize server SSLContext", e);
        }
    }

    protected SslContext buildClientSslContext(SSLContextConfig config) {
        try {
            SslProvider sslProvider = defaultSslProvider();
            SslContextBuilder sslCtxBuilder = SslContextBuilder
                .forClient()
                .trustManager(loadFromConfDir(config.getTrustCertsFile()))
                .keyManager(loadFromConfDir(config.getCertFile()), loadFromConfDir(config.getKeyFile()))
                .sslProvider(sslProvider);
            if (sslProvider == SslProvider.JDK) {
                sslCtxBuilder.sslContextProvider(findJdkProvider());
            }
            return sslCtxBuilder.build();
        } catch (Throwable e) {
            throw new RuntimeException("Fail to initialize client SSLContext", e);
        }
    }


    private SslProvider defaultSslProvider() {
        if (OpenSsl.isAvailable()) {
            return SslProvider.OPENSSL;
        }
        Provider jdkProvider = findJdkProvider();
        if (jdkProvider != null) {
            return SslProvider.JDK;
        }
        throw new IllegalStateException("Could not find TLS provider");
    }

    private Provider findJdkProvider() {
        Provider[] providers = Security.getProviders("SSLContext.TLS");
        if (providers.length > 0) {
            return providers[0];
        }
        return null;
    }


    @Override
    public void start() {
        new UptimeMetrics().bindTo(Metrics.globalRegistry);
        new ProcessorMetrics().bindTo(Metrics.globalRegistry);
        new JvmInfoMetrics().bindTo(Metrics.globalRegistry);
        new ClassLoaderMetrics().bindTo(Metrics.globalRegistry);
        new JvmCompilationMetrics().bindTo(Metrics.globalRegistry);
        new JvmMemoryMetrics().bindTo(Metrics.globalRegistry);
        new JvmThreadMetrics().bindTo(Metrics.globalRegistry);
        JvmGcMetrics jvmGcMetrics = new JvmGcMetrics();
        closeableList.add(jvmGcMetrics);

        jvmGcMetrics.bindTo(Metrics.globalRegistry);
        JvmHeapPressureMetrics jvmHeapPressureMetrics = new JvmHeapPressureMetrics();
        jvmHeapPressureMetrics.bindTo(Metrics.globalRegistry);
        closeableList.add(jvmHeapPressureMetrics);
        new NettyAllocatorMetrics(PooledByteBufAllocator.DEFAULT).bindTo(Metrics.globalRegistry);
        new NettyAllocatorMetrics(UnpooledByteBufAllocator.DEFAULT).bindTo(Metrics.globalRegistry);
    }

    @Override
    public void stop() {
        closeableList.forEach(closable -> {
            try {
                closable.close();
            } catch (Exception e) {

            }
        });
    }
}
