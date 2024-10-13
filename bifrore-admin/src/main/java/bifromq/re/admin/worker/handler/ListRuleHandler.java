package bifromq.re.admin.worker.handler;

import bifromq.re.admin.worker.http.ListRuleHttpResponse;
import bifromq.re.baserpc.clock.HLC;
import bifromq.re.router.client.IRouterClient;
import bifromq.re.router.rpc.proto.ListRuleRequest;
import bifromq.re.router.rpc.proto.ListRuleResponse;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.POST;
import javax.ws.rs.Path;
import java.util.Optional;

@Path("/list/rule")
@Slf4j
public class ListRuleHandler extends AbstractHandler {

    public ListRuleHandler(IRouterClient routerClient) {
        super(routerClient);
    }

    @POST
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
                        Optional<String> jsonResponse = buildJsonSting(response);
                        if (jsonResponse.isPresent()) {
                            ctx.response().
                                    setStatusCode(HttpResponseStatus.OK.code())
                                    .end(jsonResponse.get());
                        }else {
                            ctx.response()
                                    .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                    .end("List rule failed, error: JsonProcessingException");
                        }
                    }
                });
    }
}
