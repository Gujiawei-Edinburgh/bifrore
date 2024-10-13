package bifromq.re.admin.worker.handler;

import bifromq.re.admin.worker.http.AddRuleHttpRequest;
import bifromq.re.admin.worker.http.AddRuleHttpResponse;
import bifromq.re.baserpc.clock.HLC;
import bifromq.re.router.client.IRouterClient;
import bifromq.re.router.rpc.proto.AddRuleRequest;
import bifromq.re.router.rpc.proto.AddRuleResponse;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.PUT;
import javax.ws.rs.Path;
import java.util.Optional;

@Path("/add/rule")
@Slf4j
public class AddRuleHandler extends AbstractHandler {

    public AddRuleHandler(IRouterClient routerClient) {
        super(routerClient);
    }

    @PUT
    @Override
    public void handle(RoutingContext ctx) {
        ctx.request().bodyHandler(body -> {
            String bodyContent = body.toString();
            try {
                AddRuleHttpRequest request = objectMapper.readValue(bodyContent, AddRuleHttpRequest.class);;
                routerClient.addRule(AddRuleRequest.newBuilder()
                                .setReqId(HLC.INST.get())
                                .setRule(request.expression())
                                .addAllDestinations(request.destinations())
                                .build())
                        .whenComplete((v, e) -> {
                            if (e != null) {
                                ctx.response()
                                        .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                        .end("Add rule failed, error: " + e.getMessage());
                            } else if (v.getCode() == AddRuleResponse.Code.OK) {
                                AddRuleHttpResponse response = new AddRuleHttpResponse(v.getRuleId());
                                Optional<String> jsonResponse = buildJsonSting(response);
                                if (jsonResponse.isPresent()) {
                                    ctx.response()
                                            .setStatusCode(HttpResponseStatus.OK.code())
                                            .end(jsonResponse.get());
                                }else {
                                    ctx.response()
                                            .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                            .end("Add rule failed, error: JsonProcessingException");
                                }
                            }else {
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
