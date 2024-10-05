package bifromq.re.baserpc;

import io.grpc.Context;
import lombok.Getter;
import lombok.Setter;

import java.util.Map;

public class RPCContext {

    @Setter
    @Getter
    public static class ServerSelection {
        private String serverId;
    }

    public static final Context.Key<Map<String, String>> CUSTOM_METADATA_CTX_KEY =
            Context.key("CustomMetadata");
}
