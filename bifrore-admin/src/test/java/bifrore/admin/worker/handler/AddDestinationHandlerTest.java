package bifrore.admin.worker.handler;

import bifrore.admin.worker.MockableTest;
import bifrore.processor.rpc.proto.AddDestinationResponse;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.core.Handler;
import io.vertx.core.buffer.Buffer;
import io.vertx.core.json.JsonObject;
import org.testng.annotations.Test;

import java.util.concurrent.CompletableFuture;

import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

public class AddDestinationHandlerTest extends MockableTest {
    private final AddDestinationResponse ok = AddDestinationResponse.newBuilder()
            .setReqId(1)
            .setCode(AddDestinationResponse.Code.OK)
            .setDestinationId("testDestinationId")
            .build();
    private final AddDestinationResponse failed = AddDestinationResponse.newBuilder()
            .setReqId(2)
            .setCode(AddDestinationResponse.Code.ERROR)
            .setReason("testReason")
            .build();
    private final String cfgStr = JsonObject.of("attr", "value").encode();

    @Test
    public void testAddDestinationOk() {
        AddDestinationHandler handler = new AddDestinationHandler(processorClient);
        when(processorClient.addDestination(any())).thenReturn(CompletableFuture.completedFuture(ok));
        String payload = JsonObject.of("destinationType", "kafka", "cfg", new JsonObject(cfgStr)).encode();
        when(request.bodyHandler(any())).thenAnswer(invocation -> {
            ((Handler<Buffer>) invocation.getArguments()[0]).handle(Buffer.buffer(payload));
            return null;
        });
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.OK.code());
        verify(response).end(JsonObject.of("destinationId", ok.getDestinationId()).encode());
    }

    @Test
    public void testAddDestinationFailed() {
        AddDestinationHandler handler = new AddDestinationHandler(processorClient);
        when(processorClient.addDestination(any())).thenReturn(CompletableFuture.completedFuture(failed));
        String payload = JsonObject.of("destinationType", "kafka", "cfg", new JsonObject(cfgStr)).encode();
        when(request.bodyHandler(any())).thenAnswer(invocation -> {
            ((Handler<Buffer>) invocation.getArguments()[0]).handle(Buffer.buffer(payload));
            return null;
        });
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code());
        verify(response).end("Add destination failed, error: " + failed.getReason());
    }
}
