package bifromq.re.starter;

import bifromq.re.admin.worker.IAdminServer;
import bifromq.re.admin.worker.handler.AddRuleHandler;
import bifromq.re.admin.worker.handler.DeleteRuleHandler;
import bifromq.re.admin.worker.handler.ListRuleHandler;
import bifromq.re.baserpc.*;
import bifromq.re.processor.client.IProcessorClient;
import bifromq.re.processor.server.IProcessorServer;
import bifromq.re.processor.worker.IProcessorWorker;
import bifromq.re.processor.worker.ProcessorWorkerBuilder;
import bifromq.re.router.client.IRouterClient;
import bifromq.re.router.server.IRouterServer;
import bifromq.re.starter.config.StandaloneConfig;
import bifromq.re.starter.config.model.ClusterConfig;
import bifromq.re.starter.utils.ConfigUtil;
import bifromq.re.starter.utils.ResourceUtil;
import com.hazelcast.cluster.MembershipEvent;
import com.hazelcast.cluster.MembershipListener;
import com.hazelcast.config.Config;
import com.hazelcast.config.JoinConfig;
import com.hazelcast.config.NetworkConfig;
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
    private IRPCServer rpcServer;
    private Vertx vertx;
    private PluginManager pluginManager;
    private IProcessorClient processorClient;
    private IRouterClient routerClient;
    private IProcessorWorker processorWorker;
    private IProcessorServer processorServer;
    private IRouterServer routerServer;
    private IAdminServer adminServer;

    @Override
    protected void init(StandaloneConfig config) {
        printConfig(config);

        pluginManager = new DefaultPluginManager();
        pluginManager.loadPlugins();
        pluginManager.startPlugins();
        HazelcastInstance hz = buildHazelcastInstance(config.getClusterConfig());
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
                .setWorkerPoolSize(2);
        vertx = Vertx.vertx(vertxOptions);

        SslContext clientSslContext = config.getRpcClientConfig().getSslConfig() != null ?
                buildClientSslContext(config.getRpcClientConfig().getSslConfig()) : null;
        SslContext serverSslContext = config.getRpcClientConfig().getSslConfig() != null ?
                buildServerSslContext(config.getRpcServerConfig().getSslConfig()) : null;

        RPCServerBuilder rpcServerBuilder = IRPCServer.newBuilder()
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

        processorClient = IProcessorClient.newBuilder()
                .rpcClientBuilder(rpcClientBuilder)
                .build();
        routerClient = IRouterClient.newBuilder()
                .rpcClientBuilder(rpcClientBuilder)
                .build();

        ProcessorWorkerBuilder processorWorkerBuilder = IProcessorWorker.newBuilder()
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
                .pluginManager(pluginManager);
        processorWorker = processorWorkerBuilder.build();
        processorServer = IProcessorServer.newBuilder()
                .processorWorker(processorWorker)
                .rpcServerBuilder(rpcServerBuilder)
                .build();

        routerServer = IRouterServer.newBuilder()
                .idMap(hz.getMap("idMap"))
                .topicFilterMap(hz.getMap("topicFilterMap"))
                .processorClient(processorClient)
                .rpcServerBuilder(rpcServerBuilder)
                .build();

        adminServer = IAdminServer.newBuilder()
                .port(config.getAdminServerPort())
                .vertx(vertx)
                .addHandler(new AddRuleHandler(routerClient))
                .addHandler(new DeleteRuleHandler(routerClient))
                .addHandler(new ListRuleHandler(routerClient))
                .build();
        rpcServer = rpcServerBuilder.build();
    }

    @Override
    protected Class<StandaloneConfig> configClass() {
        return StandaloneConfig.class;
    }

    public void start() {
        super.start();
        rpcServer.start();
        processorWorker.start();
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

    private HazelcastInstance buildHazelcastInstance(ClusterConfig clusterConfig) {
        Config config = new Config();
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
