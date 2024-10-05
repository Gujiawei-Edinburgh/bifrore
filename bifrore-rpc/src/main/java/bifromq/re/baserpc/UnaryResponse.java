package bifromq.re.baserpc;

import io.grpc.stub.StreamObserver;

import java.util.Map;
import java.util.concurrent.CompletionStage;
import java.util.function.Function;

public final class UnaryResponse {

    public static <Resp> void response(Function<Map<String, String>, CompletionStage<Resp>> reqHandler,
                                       StreamObserver<Resp> observer) {
        Map<String, String> metadata = RPCContext.CUSTOM_METADATA_CTX_KEY.get();
        reqHandler.apply(metadata)
            .whenComplete((v, e) -> {
                if (e != null) {
                    observer.onError(e);
                } else {
                    observer.onNext(v);
                    observer.onCompleted();
                }
            });
    }
}
