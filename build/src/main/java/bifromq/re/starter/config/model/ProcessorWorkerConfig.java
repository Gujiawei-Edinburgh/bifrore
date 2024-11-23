package bifromq.re.starter.config.model;

import lombok.Getter;
import lombok.Setter;

@Getter
@Setter
public class ProcessorWorkerConfig {
    private int clientNum;
    private String groupName;
    private String userName;
    private String password;
    private boolean cleanStart;
    private boolean ordered;
    private long sessionExpiryInterval;
    private String host;
    private int port;
    private String clientPrefix;
}
