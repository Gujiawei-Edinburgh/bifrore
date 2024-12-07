package bifrore.baserpc;

import io.grpc.MethodDescriptor;
import java.util.Map;
import java.util.concurrent.CompletableFuture;

public interface IRPCClient {

    static RPCClientBuilder newBuilder() {
        return new RPCClientBuilder();
    }

    <ReqT, RespT> CompletableFuture<RespT> invoke(ReqT req,
                                                  Map<String, String> metadata,
                                                  MethodDescriptor<ReqT, RespT> methodDesc);
}
