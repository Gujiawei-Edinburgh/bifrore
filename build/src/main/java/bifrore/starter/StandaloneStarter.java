package bifrore.starter;

import bifrore.admin.worker.IAdminServer;
import bifrore.admin.worker.handler.AddDestinationHandler;
import bifrore.admin.worker.handler.AddRuleHandler;
import bifrore.admin.worker.handler.DeleteRuleHandler;
import bifrore.admin.worker.handler.ListRuleHandler;
import bifrore.baserpc.*;
import bifrore.common.store.PersistentMapStore;
import bifrore.common.type.SerializationUtil;
import bifrore.processor.client.IProcessorClient;
import bifrore.processor.server.IProcessorServer;
import bifrore.processor.worker.IProcessorWorker;
import bifrore.processor.worker.ProcessorWorkerBuilder;
import bifrore.router.client.IRouterClient;
import bifrore.router.server.IRouterServer;
import bifrore.starter.config.StandaloneConfig;
import bifrore.starter.config.model.ClusterConfig;
import bifrore.starter.utils.ConfigUtil;
import bifrore.starter.utils.ResourceUtil;
import com.hazelcast.cluster.MembershipEvent;
import com.hazelcast.cluster.MembershipListener;
import com.hazelcast.config.Config;
import com.hazelcast.config.MapConfig;
import com.hazelcast.config.MapStoreConfig;
import com.hazelcast.config.JoinConfig;
import com.hazelcast.config.NetworkConfig;
import com.hazelcast.core.Hazelcast;
import com.hazelcast.core.HazelcastInstance;
import com.sun.net.httpserver.HttpServer;
import io.micrometer.core.instrument.Meter;
import io.micrometer.core.instrument.Metrics;
import io.micrometer.core.instrument.binder.jvm.ExecutorServiceMetrics;
import io.micrometer.core.instrument.binder.netty4.NettyEventExecutorMetrics;
import io.micrometer.core.instrument.config.MeterFilter;
import io.micrometer.core.instrument.distribution.DistributionStatisticConfig;
import io.micrometer.prometheus.PrometheusConfig;
import io.micrometer.prometheus.PrometheusMeterRegistry;
import io.netty.channel.EventLoopGroup;
import io.netty.handler.ssl.SslContext;
import io.vertx.core.Vertx;
import io.vertx.core.VertxOptions;
import lombok.extern.slf4j.Slf4j;
import org.pf4j.DefaultPluginManager;
import org.pf4j.PluginManager;
import org.rocksdb.RocksDBException;

import java.io.IOException;
import java.io.OutputStream;
import java.lang.management.ManagementFactory;
import java.net.InetSocketAddress;
import java.time.Duration;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.LinkedTransferQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.function.Function;

@Slf4j
public class StandaloneStarter extends BaseStarter {
    private IRPCServer rpcServer;
    private PluginManager pluginManager;
    private IProcessorWorker processorWorker;
    private Thread promExporterPortThread;

    @Override
    protected void init(StandaloneConfig config) {
        String nodeId = UUID.randomUUID().toString();
        printConfig(config);

        pluginManager = new DefaultPluginManager();
        pluginManager.loadPlugins();
        pluginManager.startPlugins();
        HazelcastInstance hz;
        try {
            hz = buildHazelcastInstance(config.getClusterConfig());
        }catch (RocksDBException e) {
            throw new RuntimeException(e);
        }
        hz.getCluster().addMembershipListener(new MembershipListener() {
            @Override
            public void memberAdded(MembershipEvent membershipEvent) {
                log.info("New member joined: {}", membershipEvent.getMember());
            }

            @Override
            public void memberRemoved(MembershipEvent membershipEvent) {
                log.info("Member left: {}", membershipEvent.getMember());
            }
        });
        IClusterManager clusterManager = IClusterManager.newBuilder()
                .servers(hz.getSet("servers"))
                .build();

        ExecutorService ioClientExecutor = ExecutorServiceMetrics.monitor(Metrics.globalRegistry,
                new ThreadPoolExecutor(config.getRpcClientConfig().getWorkerThreads(),
                        config.getRpcClientConfig().getWorkerThreads(), 0L,
                        TimeUnit.MILLISECONDS, new LinkedTransferQueue<>(),
                        ResourceUtil.newThreadFactory("rpc-client-executor")), "rpc-client-executor");
        ExecutorService ioServerExecutor = ExecutorServiceMetrics.monitor(Metrics.globalRegistry,
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
                .setWorkerPoolSize(2);
        Vertx vertx = Vertx.vertx(vertxOptions);

        SslContext clientSslContext = config.getRpcClientConfig().getSslConfig() != null ?
                buildClientSslContext(config.getRpcClientConfig().getSslConfig()) : null;
        SslContext serverSslContext = config.getRpcClientConfig().getSslConfig() != null ?
                buildServerSslContext(config.getRpcServerConfig().getSslConfig()) : null;

        RPCServerBuilder rpcServerBuilder = IRPCServer.newBuilder()
                .id(nodeId)
                .host(config.getRpcServerConfig().getHost())
                .port(config.getRpcServerConfig().getPort())
                .bossEventLoopGroup(rpcServerBossELG)
                .workerEventLoopGroup(ioRPCWorkerELG)
                .sslContext(serverSslContext)
                .executor(ioServerExecutor)
                .clusterManager(clusterManager);
        RPCClientBuilder rpcClientBuilder = IRPCClient.newBuilder()
                .executor(ioClientExecutor)
                .sslContext(clientSslContext)
                .clusterManager(clusterManager);

        IProcessorClient processorClient = IProcessorClient.newBuilder()
                .rpcClientBuilder(rpcClientBuilder)
                .build();
        IRouterClient routerClient = IRouterClient.newBuilder()
                .rpcClientBuilder(rpcClientBuilder)
                .build();

        ProcessorWorkerBuilder processorWorkerBuilder = IProcessorWorker.newBuilder()
                .nodeId(nodeId)
                .clientNum(config.getProcessorWorkerConfig().getClientNum())
                .groupName(config.getProcessorWorkerConfig().getGroupName())
                .userName(config.getProcessorWorkerConfig().getUserName())
                .password(config.getProcessorWorkerConfig().getPassword())
                .cleanStart(config.getProcessorWorkerConfig().isCleanStart())
                .ordered(config.getProcessorWorkerConfig().isOrdered())
                .sessionExpiryInterval(config.getProcessorWorkerConfig().getSessionExpiryInterval())
                .host(config.getProcessorWorkerConfig().getBrokerHost())
                .port(config.getProcessorWorkerConfig().getBrokerPort())
                .clientPrefix(config.getProcessorWorkerConfig().getClientPrefix())
                .routerClient(routerClient)
                .callerCfgs(hz.getMap("callerCfgs"))
                .pluginManager(pluginManager);
        processorWorker = processorWorkerBuilder.build();
        IProcessorServer.newBuilder()
                .processorWorker(processorWorker)
                .rpcServerBuilder(rpcServerBuilder)
                .build();

        IRouterServer.newBuilder()
                .idMap(hz.getMap("idMap"))
                .topicFilterMap(hz.getMap("topicFilterMap"))
                .processorClient(processorClient)
                .rpcServerBuilder(rpcServerBuilder)
                .build();

        IAdminServer.newBuilder()
                .port(config.getAdminServerPort())
                .vertx(vertx)
                .addHandler(new AddRuleHandler(routerClient))
                .addHandler(new DeleteRuleHandler(routerClient))
                .addHandler(new ListRuleHandler(routerClient))
                .addHandler(new AddDestinationHandler(processorClient))
                .build();
        rpcServer = rpcServerBuilder.build();

        try {
            PrometheusMeterRegistry registry = new PrometheusMeterRegistry(PrometheusConfig.DEFAULT);
            registry.config().meterFilter(new MeterFilter() {
                @Override
                public DistributionStatisticConfig configure(Meter.Id id, DistributionStatisticConfig config) {
                    return DistributionStatisticConfig.builder()
                            .expiry(Duration.ofSeconds(5))
                            .build().merge(config);
                }
            });
            Metrics.addRegistry(registry);
            HttpServer prometheusExportServer =
                    HttpServer.create(new InetSocketAddress(config.getPromExporterPort()), 0);
            prometheusExportServer.createContext("/metrics", httpExchange -> {
                String response = registry.scrape();
                httpExchange.sendResponseHeaders(200, response.getBytes().length);
                try (OutputStream os = httpExchange.getResponseBody()) {
                    os.write(response.getBytes());
                }
            });
            promExporterPortThread = new Thread(prometheusExportServer::start);
        } catch (IOException e) {
            throw new RuntimeException(e);
        }
    }

    @Override
    protected Class<StandaloneConfig> configClass() {
        return StandaloneConfig.class;
    }

    public void start() {
        super.start();
        rpcServer.start();
        processorWorker.start();
        promExporterPortThread.start();
        log.info("Standalone rule engine started");
    }

    public void stop() {
        rpcServer.shutdown();
        pluginManager.stopPlugins();
        log.info("Standalone rule engine stopped");
        super.stop();
    }

    private void printConfig(StandaloneConfig config) {
        List<String> arguments = ManagementFactory.getRuntimeMXBean().getInputArguments();
        log.info("JVM arguments: \n  {}", String.join("\n  ", arguments));
        log.info("Config(YAML): \n{}", ConfigUtil.serialize(config));
    }

    private HazelcastInstance buildHazelcastInstance(ClusterConfig clusterConfig) throws RocksDBException {
        Config config = new Config();
        MapStoreConfig idMapStoreConfig = new MapStoreConfig()
                .setImplementation(new PersistentMapStore<>("idMap", "idMap",
                        String::getBytes, String::new, b -> b, b -> b))
                .setWriteDelaySeconds(0);
        MapConfig idMapConfig = new MapConfig("idMap")
                .setMapStoreConfig(idMapStoreConfig);
        config.addMapConfig(idMapConfig);

        MapStoreConfig callerCfgStoreConfig = new MapStoreConfig()
                .setImplementation(new PersistentMapStore<>("callerCfgs", "callerCfgs",
                        String::getBytes, String::new, b -> b, b -> b))
                .setWriteDelaySeconds(0);
        MapConfig callerCfgConfig = new MapConfig("callerCfgs")
                .setMapStoreConfig(callerCfgStoreConfig);
        config.addMapConfig(callerCfgConfig);

        MapStoreConfig tfsStoreConfig = new MapStoreConfig()
                .setImplementation(new PersistentMapStore<>("topicFilterMap", "topicFilterMap",
                        String::getBytes, String::new,
                        SerializationUtil::serializeList, bytes -> {
                            if (bytes == null) {
                                return null;
                            }
                            return SerializationUtil.deserializeList(bytes);
                        }))
                .setWriteDelaySeconds(0);
        MapConfig tfsConfig = new MapConfig("topicFilterMap")
                .setMapStoreConfig(tfsStoreConfig);
        config.addMapConfig(tfsConfig);

        config.setClusterName(clusterConfig.getClusterName());
        NetworkConfig networkConfig = config.getNetworkConfig();
        networkConfig.setPort(clusterConfig.getPort())
                .setPortAutoIncrement(true);
        JoinConfig joinConfig = networkConfig.getJoin();
        joinConfig.getMulticastConfig().setEnabled(false);
        joinConfig.getTcpIpConfig()
                .setEnabled(true)
                .addMember(clusterConfig.getMemberAddress());
        return Hazelcast.newHazelcastInstance(config);
    }

    public static void main(String[] args) {
        StarterRunner.run(StandaloneStarter.class, args);
    }
}
