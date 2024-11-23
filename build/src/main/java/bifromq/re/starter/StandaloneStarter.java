package bifromq.re.starter;

import bifromq.re.baserpc.IRPCServer;
import bifromq.re.baserpc.RPCServerBuilder;
import bifromq.re.processor.client.IProcessorClient;
import bifromq.re.processor.server.IProcessorServer;
import bifromq.re.processor.worker.IProcessorWorker;
import bifromq.re.processor.worker.ProcessorWorkerBuilder;
import bifromq.re.router.client.IRouterClient;
import bifromq.re.router.server.IRouterServer;
import bifromq.re.starter.config.StandaloneConfig;
import bifromq.re.starter.utils.ConfigUtil;
import bifromq.re.starter.utils.ResourceUtil;
import com.hazelcast.core.Hazelcast;
import com.hazelcast.core.HazelcastInstance;
import io.micrometer.core.instrument.Metrics;
import io.micrometer.core.instrument.binder.jvm.ExecutorServiceMetrics;
import io.micrometer.core.instrument.binder.netty4.NettyEventExecutorMetrics;
import io.netty.channel.EventLoopGroup;
import io.netty.handler.ssl.SslContext;
import io.vertx.core.Vertx;
import io.vertx.core.VertxOptions;
import lombok.extern.slf4j.Slf4j;
import org.pf4j.DefaultPluginManager;
import org.pf4j.PluginManager;

import java.lang.management.ManagementFactory;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.LinkedTransferQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

@Slf4j
public class StandaloneStarter extends BaseStarter {
    private ExecutorService ioClientExecutor;
    private ExecutorService ioServerExecutor;
    private Vertx vertx;

    @Override
    protected void init(StandaloneConfig config) {
        printConfig(config);

        PluginManager pluginManager = new DefaultPluginManager();
        HazelcastInstance hz = buildHazelcastInstance();

        ioClientExecutor = ExecutorServiceMetrics.monitor(Metrics.globalRegistry,
                new ThreadPoolExecutor(config.getRpcClientConfig().getWorkerThreads(),
                        config.getRpcClientConfig().getWorkerThreads(), 0L,
                        TimeUnit.MILLISECONDS, new LinkedTransferQueue<>(),
                        ResourceUtil.newThreadFactory("rpc-client-executor")), "rpc-client-executor");
        ioServerExecutor = ExecutorServiceMetrics.monitor(Metrics.globalRegistry,
                new ThreadPoolExecutor(config.getRpcServerConfig().getWorkerThreads(),
                        config.getRpcServerConfig().getWorkerThreads(), 0L,
                        TimeUnit.MILLISECONDS, new LinkedTransferQueue<>(),
                        ResourceUtil.newThreadFactory("rpc-server-executor")), "rpc-server-executor");

        EventLoopGroup rpcServerBossELG = ResourceUtil.createEventLoopGroup(1,
                ResourceUtil.newThreadFactory("rpc-boss-elg"));
        new NettyEventExecutorMetrics(rpcServerBossELG).bindTo(Metrics.globalRegistry);
        EventLoopGroup ioRPCWorkerELG = ResourceUtil.createEventLoopGroup(0,
                ResourceUtil.newThreadFactory("io-rpc-worker-elg"));
        new NettyEventExecutorMetrics(ioRPCWorkerELG).bindTo(Metrics.globalRegistry);

        VertxOptions vertxOptions = new VertxOptions();
        vertxOptions.setEventLoopPoolSize(1)
                .setWorkerPoolSize(1);
        vertx = Vertx.vertx(vertxOptions);

        RPCServerBuilder rpcServerBuilder = IRPCServer.newBuilder()
                .host(config.getRpcServerConfig().getHost())
                .port(config.getRpcServerConfig().getPort())
                .bossEventLoopGroup(rpcServerBossELG)
                .workerEventLoopGroup(ioRPCWorkerELG)
                .executor(ioServerExecutor);

        SslContext clientSslContext = config.getRpcClientConfig().getSslConfig() != null ?
                buildClientSslContext(config.getRpcClientConfig().getSslConfig()) : null;

        IProcessorClient processorClient = IProcessorClient.newBuilder()
                .executor(ioClientExecutor)
                .sslContext(clientSslContext)
                .build();
        IRouterClient routerClient = IRouterClient.newBuilder()
                .executor(ioServerExecutor)
                .sslContext(clientSslContext)
                .build();

        ProcessorWorkerBuilder processorWorkerBuilder = IProcessorWorker.newBuilder()
                .clientNum(config.getProcessorWorkerConfig().getClientNum())
                .groupName(config.getProcessorWorkerConfig().getGroupName())
                .userName(config.getProcessorWorkerConfig().getUserName())
                .password(config.getProcessorWorkerConfig().getPassword())
                .cleanStart(config.getProcessorWorkerConfig().isCleanStart())
                .ordered(config.getProcessorWorkerConfig().isOrdered())
                .sessionExpiryInterval(config.getProcessorWorkerConfig().getSessionExpiryInterval())
                .host(config.getProcessorWorkerConfig().getHost())
                .port(config.getProcessorWorkerConfig().getPort())
                .clientPrefix(config.getProcessorWorkerConfig().getClientPrefix())
                .routerClient(routerClient)
                .pluginManager(pluginManager);
        IProcessorServer processorServer = IProcessorServer.newBuilder()
                .processorWorkerBuilder(processorWorkerBuilder)
                .rpcServerBuilder(rpcServerBuilder)
                .build();

        IRouterServer routerServer = IRouterServer.newBuilder()
                .idMap(hz.getMap("idMap"))
                .topicFilterMap(hz.getMap("topicFilterMap"))
                .processorClient(processorClient)
                .rpcServerBuilder(rpcServerBuilder)
                .build();
    }

    @Override
    protected Class<StandaloneConfig> configClass() {
        return StandaloneConfig.class;
    }

    public void start() {
        super.start();
        log.info("Standalone rule engine started");
    }

    public void stop() {
        log.info("Standalone rule engine stopped");
        super.stop();
    }

    private void printConfig(StandaloneConfig config) {
        List<String> arguments = ManagementFactory.getRuntimeMXBean().getInputArguments();
        log.info("JVM arguments: \n  {}", String.join("\n  ", arguments));
        log.info("Config(YAML): \n{}", ConfigUtil.serialize(config));
    }

    private HazelcastInstance buildHazelcastInstance() {
        return Hazelcast.newHazelcastInstance();
    }

    public static void main(String[] args) {
        StarterRunner.run(StandaloneStarter.class, args);
    }
}
