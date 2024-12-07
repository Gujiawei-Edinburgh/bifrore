package bifrore.starter.config;

import bifrore.starter.config.model.ClusterConfig;
import bifrore.starter.config.model.ProcessorWorkerConfig;
import bifrore.starter.config.model.RPCClientConfig;
import bifrore.starter.config.model.RPCServerConfig;
import com.fasterxml.jackson.annotation.JsonSetter;
import com.fasterxml.jackson.annotation.Nulls;
import lombok.Getter;
import lombok.Setter;

@Getter
@Setter
public class StandaloneConfig {
    private ClusterConfig clusterConfig;
    private int adminServerPort;
    @JsonSetter(nulls = Nulls.SKIP)
    private RPCClientConfig rpcClientConfig = new RPCClientConfig();
    @JsonSetter(nulls = Nulls.SKIP)
    private RPCServerConfig rpcServerConfig = new RPCServerConfig();
    private ProcessorWorkerConfig processorWorkerConfig = new ProcessorWorkerConfig();
}
