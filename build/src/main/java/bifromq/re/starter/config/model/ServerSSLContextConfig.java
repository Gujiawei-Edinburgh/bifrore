package bifromq.re.starter.config.model;

import io.netty.handler.ssl.ClientAuth;
import lombok.Getter;
import lombok.Setter;

@Getter
@Setter
public class ServerSSLContextConfig {
    private String certFile;
    private String keyFile;
    private String trustCertsFile;
    private String clientAuth = ClientAuth.OPTIONAL.name();
}
