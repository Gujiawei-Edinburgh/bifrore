package bifromq.re.starter.config.model;

import lombok.Getter;
import lombok.Setter;

@Getter
@Setter
public class ClusterConfig {
    private boolean bootstrap;
    private String clusterName;
    private int port ;
    private String memberAddress;
}
