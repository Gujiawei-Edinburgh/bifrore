package bifromq.re.router.server;

import bifromq.re.baserpc.RPCServerBuilder;
import bifromq.re.processor.client.IProcessorClient;
import com.google.common.base.Preconditions;

import java.util.List;
import java.util.Map;

public final class RouterServerBuilder {
    Map<String, byte[]> idMap;
    Map<String, List<byte[]>> topicFilterMap;
    IProcessorClient processorClient;
    RPCServerBuilder rpcServerBuilder;

    public RouterServerBuilder idMap(Map<String, byte[]> idMap) {
        this.idMap = idMap;
        return this;
    }

    public RouterServerBuilder topicFilterMap(Map<String, List<byte[]>> topicFilterMap) {
        this.topicFilterMap = topicFilterMap;
        return this;
    }

    public RouterServerBuilder processorClient(IProcessorClient processorClient) {
        this.processorClient = processorClient;
        return this;
    }

    public RouterServerBuilder rpcServerBuilder(RPCServerBuilder rpcServerBuilder) {
        this.rpcServerBuilder = rpcServerBuilder;
        return this;
    }

    public IRouterServer build() {
        Preconditions.checkNotNull(idMap, "idMap is null");
        Preconditions.checkNotNull(topicFilterMap, "topicFilterMap is null");
        Preconditions.checkNotNull(rpcServerBuilder, "RPC Server Builder is null");
        return new RouterServer(this);
    }
}
