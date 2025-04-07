package bifrore.admin.worker.handler;

import bifrore.admin.worker.MockableTest;
import bifrore.router.rpc.proto.AddRuleResponse;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.core.Handler;
import io.vertx.core.buffer.Buffer;
import io.vertx.core.json.JsonObject;
import org.testng.annotations.Test;

import java.util.concurrent.CompletableFuture;

import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

public class AddRuleHandlerTest extends MockableTest {

    private final AddRuleResponse ok = AddRuleResponse.newBuilder()
            .setReqId(1)
            .setCode(AddRuleResponse.Code.OK)
            .setRuleId("testRuleId")
            .build();

    private final AddRuleResponse failed = AddRuleResponse.newBuilder()
            .setReqId(2)
            .setCode(AddRuleResponse.Code.ERROR)
            .setFailReason("testFailReason")
            .build();

    @Test
    public void testAddRuleOk() {
        AddRuleHandler handler = new AddRuleHandler(routerClient);
        when(routerClient.addRule(any())).thenReturn(CompletableFuture.completedFuture(ok));
        String jsonBody = "{\"expression\":\"test expression\",\"destinations\":[\"log/1\"]}";
        when(request.bodyHandler(any())).thenAnswer(invocation -> {
            ((Handler<Buffer>) invocation.getArguments()[0]).handle(Buffer.buffer(jsonBody));
            return null;
        });
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.OK.code());
        verify(response).end(JsonObject.of("ruleId", "testRuleId").encode());
    }

    @Test
    public void testAddRuleFailed() {
        AddRuleHandler handler = new AddRuleHandler(routerClient);
        when(routerClient.addRule(any())).thenReturn(CompletableFuture.completedFuture(failed));
        String jsonBody = "{\"expression\":\"test expression\",\"destinations\":[\"log\"]}";
        when(request.bodyHandler(any())).thenAnswer(invocation -> {
            ((Handler<Buffer>) invocation.getArguments()[0]).handle(Buffer.buffer(jsonBody));
            return null;
        });
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code());
        verify(response).end("Add rule failed, error:  " + failed.getFailReason());
    }

    @Test
    public void testAddRuleWithException() {
        AddRuleHandler handler = new AddRuleHandler(routerClient);
        CompletableFuture<AddRuleResponse> future = new CompletableFuture<>();
        future.completeExceptionally(new RuntimeException("test error"));
        when(routerClient.addRule(any())).thenReturn(future);
        String jsonBody = "{\"expression\":\"test expression\",\"destinations\":[\"log\"]}";
        when(request.bodyHandler(any())).thenAnswer(invocation -> {
            ((Handler<Buffer>) invocation.getArguments()[0]).handle(Buffer.buffer(jsonBody));
            return null;
        });
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code());
        verify(response).end("Add rule failed, error: " + "test error");
    }

    @Test
    public void testPayloadDecodeError() {
        AddRuleHandler handler = new AddRuleHandler(routerClient);
        String jsonBody = "error jsonBody";
        when(request.bodyHandler(any())).thenAnswer(invocation -> {
            ((Handler<Buffer>) invocation.getArguments()[0]).handle(Buffer.buffer(jsonBody));
            return null;
        });
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.BAD_REQUEST.code());
    }
}
