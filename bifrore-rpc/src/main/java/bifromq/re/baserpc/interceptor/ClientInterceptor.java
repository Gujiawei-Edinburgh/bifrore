package bifromq.re.baserpc.interceptor;

import bifromq.re.baserpc.RPCContext;
import io.grpc.*;
import lombok.extern.slf4j.Slf4j;

@Slf4j
public class ClientInterceptor implements io.grpc.ClientInterceptor {
    private final String serviceUniqueName;

    public ClientInterceptor() {
        this(null);
    }

    public ClientInterceptor(String serviceUniqueName) {
        this.serviceUniqueName = serviceUniqueName;
    }

    @Override
    public <ReqT, RespT> ClientCall<ReqT, RespT> interceptCall(MethodDescriptor<ReqT, RespT> method,
                                                               CallOptions callOptions, Channel next) {
        return new ForwardingClientCall.SimpleForwardingClientCall<>(next.newCall(method, callOptions)) {
            @Override
            public void start(Listener<RespT> responseListener, Metadata headers) {

                super.start(responseListener, headers);
            }
        };
    }
}
