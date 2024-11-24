package bifromq.re.starter.config;

import bifromq.re.starter.config.model.ProcessorWorkerConfig;
import bifromq.re.starter.config.model.RPCClientConfig;
import bifromq.re.starter.config.model.RPCServerConfig;
import com.fasterxml.jackson.annotation.JsonSetter;
import com.fasterxml.jackson.annotation.Nulls;
import lombok.Getter;
import lombok.Setter;

@Getter
@Setter
public class StandaloneConfig {
    private boolean bootstrap;
    private int adminServerPort;
    @JsonSetter(nulls = Nulls.SKIP)
    private RPCClientConfig rpcClientConfig = new RPCClientConfig();
    @JsonSetter(nulls = Nulls.SKIP)
    private RPCServerConfig rpcServerConfig = new RPCServerConfig();
    private ProcessorWorkerConfig processorWorkerConfig = new ProcessorWorkerConfig();
}
