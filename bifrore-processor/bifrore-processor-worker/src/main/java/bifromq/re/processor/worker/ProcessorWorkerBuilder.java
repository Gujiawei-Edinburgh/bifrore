package bifromq.re.processor.worker;

import io.netty.channel.EventLoopGroup;
import io.netty.handler.ssl.SslContext;
import org.pf4j.PluginManager;

import java.util.concurrent.Executor;

public class ProcessorWorkerBuilder {
    int clientNum;
    String groupName;
    String userName;
    String password;
    boolean cleanStart;
    boolean ordered;
    long sessionExpiryInterval;
    String host;
    int port;
    String clientPrefix;
    PluginManager pluginManager;
    Executor executor;
    EventLoopGroup eventLoopGroup;
    SslContext sslContext;

    public ProcessorWorkerBuilder clientNum(int clientNum) {
        this.clientNum = clientNum;
        return this;
    }

    public ProcessorWorkerBuilder groupName(String groupName) {
        this.groupName = groupName;
        return this;
    }

    public ProcessorWorkerBuilder userName(String userName) {
        this.userName = userName;
        return this;
    }

    public ProcessorWorkerBuilder password(String password) {
        this.password = password;
        return this;
    }

    public ProcessorWorkerBuilder cleanStart(boolean cleanStart) {
        this.cleanStart = cleanStart;
        return this;
    }

    public ProcessorWorkerBuilder ordered(boolean ordered) {
        this.ordered = ordered;
        return this;
    }

    public ProcessorWorkerBuilder sessionExpiryInterval(long sessionExpiryInterval) {
        this.sessionExpiryInterval = sessionExpiryInterval;
        return this;
    }

    public ProcessorWorkerBuilder host(String host) {
        this.host = host;
        return this;
    }

    public ProcessorWorkerBuilder port(int port) {
        this.port = port;
        return this;
    }

    public ProcessorWorkerBuilder clientPrefix(String clientPrefix) {
        this.clientPrefix = clientPrefix;
        return this;
    }

    public ProcessorWorkerBuilder pluginManager(PluginManager pluginManager) {
        this.pluginManager = pluginManager;
        return this;
    }

    public ProcessorWorkerBuilder executor(Executor executor) {
        this.executor = executor;
        return this;
    }

    public ProcessorWorkerBuilder eventLoopGroup(EventLoopGroup eventLoopGroup) {
        this.eventLoopGroup = eventLoopGroup;
        return this;
    }

    public ProcessorWorkerBuilder sslContext(SslContext sslContext) {
        this.sslContext = sslContext;
        return this;
    }

    public IProcessorWorker build() {
        return new ProcessorWorker(this);
    }
}
