package bifromq.re.starter.config.model;

import lombok.Getter;
import lombok.Setter;

@Getter
@Setter
public class SSLContextConfig {
    private String certFile;
    private String keyFile;
    private String trustCertsFile;
}
