package bifromq.re.admin.worker.handler;

import bifromq.re.admin.worker.http.DeleteRuleHttpResponse;
import bifromq.re.router.client.IRouterClient;
import bifromq.re.router.rpc.proto.DeleteRuleRequest;
import bifromq.re.router.rpc.proto.DeleteRuleResponse;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.DELETE;
import javax.ws.rs.Path;
import java.util.List;
import java.util.Optional;

@Path("/rule/delete")
@Slf4j
public class DeleteRuleHandler extends AbstractHandler {

    public DeleteRuleHandler(IRouterClient routerClient) {
        super(routerClient);
    }

    @DELETE
    @Override
    public void handle(RoutingContext ctx) {
        List<String> ruleIds = ctx.queryParams().getAll("ruleId");
        if (ruleIds.isEmpty()) {
            ctx.response().
                    setStatusCode(HttpResponseStatus.OK.code())
                    .end("Delete rules ok");
            return;
        }
        routerClient.deleteRule(DeleteRuleRequest.newBuilder().build())
                .whenComplete((v, e) -> {
                    if (e != null) {
                        ctx.response()
                                .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                .end("Delete rules failed, error: " + e.getMessage());
                    } else if (v.getCode() == DeleteRuleResponse.Code.ERROR) {
                        ctx.response()
                                .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                .end("Delete rules failed, error: " + v.getFailReason());
                    }else {
                        DeleteRuleHttpResponse response = new DeleteRuleHttpResponse(true);
                        Optional<String> jsonResponse = buildJsonSting(response);
                        if (jsonResponse.isPresent()) {
                            ctx.response().
                                    setStatusCode(HttpResponseStatus.OK.code())
                                    .end(jsonResponse.get());
                        }else {
                            ctx.response()
                                    .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                    .end("Delete rule failed, error: JsonProcessingException");
                        }
                    }
                });
    }
}
