package bifrore.starter.config.model;

import com.fasterxml.jackson.annotation.JsonSetter;
import com.fasterxml.jackson.annotation.Nulls;
import lombok.Getter;
import lombok.Setter;

@Getter
@Setter
public class RPCClientConfig {
    private int workerThreads = Math.max(2, Runtime.getRuntime().availableProcessors() / 8);
    @JsonSetter(nulls = Nulls.SKIP)
    private SSLContextConfig sslConfig;
}
