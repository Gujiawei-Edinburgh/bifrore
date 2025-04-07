package bifrore.admin.worker.handler;

import bifrore.admin.worker.MockableTest;
import bifrore.router.rpc.proto.DeleteRuleResponse;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.core.MultiMap;
import org.testng.annotations.Test;
import java.util.concurrent.CompletableFuture;

import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

public class DeleteRuleHandlerTest extends MockableTest {

    private final DeleteRuleResponse ok = DeleteRuleResponse.newBuilder()
            .setReqId(1)
            .setCode(DeleteRuleResponse.Code.OK)
            .build();

    private final DeleteRuleResponse failed = DeleteRuleResponse.newBuilder()
            .setReqId(2)
            .setCode(DeleteRuleResponse.Code.ERROR)
            .setFailReason("testFailReason")
            .build();

    @Test
    public void testDeleteRuleOk() {
        DeleteRuleHandler handler = new DeleteRuleHandler(routerClient);
        when(routerClient.deleteRule(any())).thenReturn(CompletableFuture.completedFuture(ok));
        MultiMap queryParams = MultiMap.caseInsensitiveMultiMap();
        queryParams.add("ruleId", "r1");
        when(ctx.queryParams()).thenReturn(queryParams);
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.OK.code());
    }

    @Test
    public void testDeleteEmptyRule() {
        DeleteRuleHandler handler = new DeleteRuleHandler(routerClient);
        MultiMap queryParams = MultiMap.caseInsensitiveMultiMap();
        when(ctx.queryParams()).thenReturn(queryParams);
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.OK.code());
    }

    @Test
    public void testDeleteRuleFailed() {
        DeleteRuleHandler handler = new DeleteRuleHandler(routerClient);
        when(routerClient.deleteRule(any())).thenReturn(CompletableFuture.completedFuture(failed));
        MultiMap queryParams = MultiMap.caseInsensitiveMultiMap();
        queryParams.add("ruleId", "r1");
        when(ctx.queryParams()).thenReturn(queryParams);
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code());
    }

    @Test
    public void testDeleteEmptyRuleFailed() {
        DeleteRuleHandler handler = new DeleteRuleHandler(routerClient);
        MultiMap queryParams = MultiMap.caseInsensitiveMultiMap();
        CompletableFuture<DeleteRuleResponse> future = new CompletableFuture<>();
        future.completeExceptionally(new RuntimeException("testException"));
        when(routerClient.deleteRule(any())).thenReturn(future);
        queryParams.add("ruleId", "r1");
        when(ctx.queryParams()).thenReturn(queryParams);
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code());
    }
}
