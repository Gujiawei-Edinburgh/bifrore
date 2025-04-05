package bifrore.admin.worker.handler;

import bifrore.admin.worker.http.AddRuleHttpRequest;
import bifrore.baserpc.clock.HLC;
import bifrore.monitoring.metrics.SysMeter;
import bifrore.router.client.IRouterClient;
import bifrore.router.rpc.proto.AddRuleRequest;
import bifrore.router.rpc.proto.AddRuleResponse;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.core.json.JsonObject;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.PUT;
import javax.ws.rs.Path;

import static bifrore.monitoring.metrics.SysMetric.HttpAddRuleCount;
import static bifrore.monitoring.metrics.SysMetric.HttpAddRuleFailureCount;

@Path("/rule")
@Slf4j
public class AddRuleHandler extends AbstractHandler {

    public AddRuleHandler(IRouterClient routerClient) {
        super(routerClient);
    }

    @PUT
    @Override
    public void handle(RoutingContext ctx) {
        SysMeter.INSTANCE.recordCount(HttpAddRuleCount);
        ctx.request().bodyHandler(body -> {
            String bodyContent = body.toString();
            try {
                AddRuleHttpRequest request = objectMapper.readValue(bodyContent, AddRuleHttpRequest.class);
                routerClient.addRule(AddRuleRequest.newBuilder()
                                .setReqId(HLC.INST.get())
                                .setRule(request.expression())
                                .addAllDestinations(request.destinations())
                                .build())
                        .whenComplete((v, e) -> {
                            if (e != null) {
                                SysMeter.INSTANCE.recordCount(HttpAddRuleFailureCount);
                                ctx.response()
                                        .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                        .end("Add rule failed, error: " + e.getMessage());
                            } else if (v.getCode() == AddRuleResponse.Code.OK) {
                                ctx.response()
                                        .setStatusCode(HttpResponseStatus.OK.code())
                                        .end(JsonObject.of("ruleId", v.getRuleId()).encode());
                            }else {
                                SysMeter.INSTANCE.recordCount(HttpAddRuleFailureCount);
                                ctx.response()
                                        .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                        .end("Add rule failed, error:  " + v.getFailReason());
                            }
                        });
            }catch (Exception ex) {
                log.error("Failed to handle add rule request", ex);
                ctx.response()
                        .setStatusCode(HttpResponseStatus.BAD_REQUEST.code())
                        .end("Invalid request: " + ex.getMessage());
            }
        });
    }
}
