package bifrore.baserpc.interceptor;


import io.grpc.Context;
import io.grpc.Contexts;
import io.grpc.ForwardingServerCallListener;
import io.grpc.Metadata;
import io.grpc.ServerCall;
import io.grpc.ServerCallHandler;
import io.grpc.Status;
import lombok.extern.slf4j.Slf4j;

@Slf4j
public class ServerInterceptor implements io.grpc.ServerInterceptor {
    private static final ServerCall.Listener NOOP_LISTENER = new ServerCall.Listener<>() {
    };

    public ServerInterceptor() {
    }

    @Override
    public <ReqT, RespT> ServerCall.Listener<ReqT> interceptCall(ServerCall<ReqT, RespT> call,
                                                                 Metadata headers,
                                                                 ServerCallHandler<ReqT, RespT> next) {
        try {
            Context ctx = Context.current();
            ServerCall.Listener<ReqT> listener = Contexts.interceptCall(ctx, call, headers, next);
            return new ForwardingServerCallListener.SimpleForwardingServerCallListener<>(listener) {
                @Override
                public void onHalfClose() {
                    try {
                        super.onHalfClose();
                    } catch (Exception e) {
                        log.error("Failed to execute server call.", e);
                        call.close(Status.INTERNAL.withCause(e).withDescription(e.getMessage()), headers);
                    }
                }
            };
        } catch (UnsupportedOperationException e) {
            log.error("Failed to determine traffic identifier from the call", e);
            call.close(Status.UNAUTHENTICATED.withDescription("Invalid Client Certificate"), headers);
            return NOOP_LISTENER;
        } catch (Throwable e) {
            log.error("Failed to make server call", e);
            call.close(Status.INTERNAL.withDescription("Server handling request error"), headers);
            return NOOP_LISTENER;
        }
    }
}
