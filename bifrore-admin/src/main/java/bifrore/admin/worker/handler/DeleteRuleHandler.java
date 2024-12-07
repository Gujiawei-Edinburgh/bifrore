package bifrore.admin.worker.handler;

import bifrore.baserpc.clock.HLC;
import bifrore.router.client.IRouterClient;
import bifrore.router.rpc.proto.DeleteRuleRequest;
import bifrore.router.rpc.proto.DeleteRuleResponse;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.DELETE;
import javax.ws.rs.Path;

@Path("/rule")
@Slf4j
public class DeleteRuleHandler extends AbstractHandler {

    public DeleteRuleHandler(IRouterClient routerClient) {
        super(routerClient);
    }

    @DELETE
    @Override
    public void handle(RoutingContext ctx) {
        String ruleId = ctx.queryParams().get("ruleId");
        if (ruleId == null || ruleId.isEmpty()) {
            ctx.response().
                    setStatusCode(HttpResponseStatus.BAD_REQUEST.code())
                    .end("RuleId is required");
            return;
        }
        DeleteRuleRequest deleteRuleRequest = DeleteRuleRequest.newBuilder()
                .setReqId(HLC.INST.get())
                .setRuleId(ruleId)
                .build();
        routerClient.deleteRule(deleteRuleRequest)
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
                        ctx.response().
                                setStatusCode(HttpResponseStatus.OK.code())
                                .end("Delete rules ok");
                    }
                });
    }
}
