package bifrore.admin.worker.handler;

import bifrore.admin.worker.MockableTest;
import bifrore.admin.worker.http.ListRuleHttpResponse;
import bifrore.router.rpc.proto.ListRuleResponse;
import bifrore.router.rpc.proto.RuleMeta;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.hubspot.jackson.datatype.protobuf.ProtobufModule;
import io.netty.handler.codec.http.HttpResponseStatus;
import org.testng.annotations.Test;

import java.util.List;
import java.util.concurrent.CompletableFuture;

import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;
import static org.testng.Assert.fail;

public class ListRuleHandlerTest extends MockableTest {

    private final List<RuleMeta> rules = List.of(RuleMeta.newBuilder()
            .setPlaintextRule("select * from a where temp > 30")
            .setTopicFilter("a")
            .addAllDestinations(List.of("log", "kafka"))
            .build());

    private final ListRuleResponse ok = ListRuleResponse.newBuilder()
            .setReqId(1)
            .setCode(ListRuleResponse.Code.OK)
            .addAllRules(rules)
            .build();

    private final ListRuleResponse failed = ListRuleResponse.newBuilder()
            .setReqId(2)
            .setCode(ListRuleResponse.Code.ERROR)
            .setFailReason("testFailReason")
            .build();

    private final ObjectMapper objectMapper = new ObjectMapper().registerModule(new ProtobufModule());

    @Test
    public void testListRuleOk() {
        ListRuleHandler handler = new ListRuleHandler(client);
        when(client.listRule(any())).thenReturn(CompletableFuture.completedFuture(ok));
        handler.handle(ctx);
        try {
            String endString = objectMapper.writeValueAsString(new ListRuleHttpResponse(ok.getRulesList()));
            verify(response).setStatusCode(HttpResponseStatus.OK.code());
            verify(response).end(endString);
        }catch (Exception e){
            fail();
        }
    }

    @Test
    public void testListRuleFailed() {
        ListRuleHandler handler = new ListRuleHandler(client);
        when(client.listRule(any())).thenReturn(CompletableFuture.completedFuture(failed));
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code());
    }

    @Test
    public void testListRuleWithException() {
        ListRuleHandler handler = new ListRuleHandler(client);
        CompletableFuture<ListRuleResponse> future = new CompletableFuture<>();
        future.completeExceptionally(new RuntimeException("testException"));
        when(client.listRule(any())).thenReturn(future);
        handler.handle(ctx);
        verify(response).setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code());
    }
}
