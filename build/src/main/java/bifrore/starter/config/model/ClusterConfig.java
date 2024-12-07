package bifrore.starter.config.model;

import lombok.Getter;
import lombok.Setter;

@Getter
@Setter
public class ClusterConfig {
    private String clusterName;
    private int port ;
    private String memberAddress;
}
