package bifrore.admin.worker.handler;

import bifrore.admin.worker.http.ListRuleHttpResponse;
import bifrore.baserpc.clock.HLC;
import bifrore.router.client.IRouterClient;
import bifrore.router.rpc.proto.ListRuleRequest;
import bifrore.router.rpc.proto.ListRuleResponse;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.GET;
import javax.ws.rs.Path;

@Path("/rule")
@Slf4j
public class ListRuleHandler extends AbstractHandler {

    public ListRuleHandler(IRouterClient routerClient) {
        super(routerClient);
    }

    @GET
    @Override
    public void handle(RoutingContext ctx) {
        routerClient.listRule(ListRuleRequest.newBuilder()
                        .setReqId(HLC.INST.get())
                        .build())
                .whenComplete((v, e) -> {
                    if (e != null) {
                        ctx.response()
                                .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                .end("List rule failed, error: " + e.getMessage());
                    } else if (v.getCode() == ListRuleResponse.Code.ERROR) {
                        ctx.response()
                                .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                .end("List rule failed, error: " + v.getFailReason());
                    }else {
                        ListRuleHttpResponse response = new ListRuleHttpResponse(v.getRulesList());
                        ctx.response().
                                setStatusCode(HttpResponseStatus.OK.code())
                                .end(response.toString());
                    }
                });
    }
}
