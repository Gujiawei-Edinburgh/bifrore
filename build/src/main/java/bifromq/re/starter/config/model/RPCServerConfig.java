package bifromq.re.starter.config.model;

import com.fasterxml.jackson.annotation.JsonSetter;
import com.fasterxml.jackson.annotation.Nulls;
import lombok.Getter;
import lombok.Setter;

@Getter
@Setter
public class RPCServerConfig {
    private String host;
    private int port = 0;
    @JsonSetter(nulls = Nulls.SKIP)
    private Integer workerThreads = Math.max(2, Runtime.getRuntime().availableProcessors() / 4);
    @JsonSetter(nulls = Nulls.SKIP)
    private ServerSSLContextConfig sslConfig;
}
