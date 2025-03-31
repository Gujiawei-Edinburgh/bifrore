package bifrore.admin.worker.handler;

import bifrore.admin.worker.http.AddDestinationHttpRequest;
import bifrore.baserpc.clock.HLC;
import bifrore.processor.client.IProcessorClient;
import bifrore.processor.rpc.proto.AddDestinationRequest;
import bifrore.processor.rpc.proto.AddDestinationResponse;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.hubspot.jackson.datatype.protobuf.ProtobufModule;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.core.Handler;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.PUT;
import javax.ws.rs.Path;

@Path("/destination")
@Slf4j
public class AddDestinationHandler implements Handler<RoutingContext> {
    final IProcessorClient processorClient;
    final ObjectMapper objectMapper = new ObjectMapper().registerModule(new ProtobufModule());

    public AddDestinationHandler(IProcessorClient processorClient) {
        this.processorClient = processorClient;
    }

    @PUT
    @Override
    public void handle(RoutingContext ctx) {
        ctx.request().bodyHandler(body -> {
            String bodyContent = body.toString();
            try {
                AddDestinationHttpRequest request = objectMapper.readValue(bodyContent, AddDestinationHttpRequest.class);
                processorClient.addDestination(AddDestinationRequest.newBuilder()
                                .setReqId(HLC.INST.get())
                                .setDestinationType(request.destinationType())
                                .putAllDestinationCfg(request.cfg())
                                .build())
                        .whenComplete((v, e) -> {
                            if (e != null) {
                                ctx.response()
                                        .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                        .end("Add destination failed, error: " + e.getMessage());
                            }else if (v.getCode() == AddDestinationResponse.Code.OK) {
                                ctx.response()
                                        .setStatusCode(HttpResponseStatus.OK.code())
                                        .end("Add destination successful, rule id: " + v.getDestinationId());
                            }else {
                                ctx.response()
                                        .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                        .end("Add destination failed, error: " + v.getReason());
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
