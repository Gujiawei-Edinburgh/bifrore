package bifrore.admin.worker.handler;

import bifrore.processor.client.IProcessorClient;
import bifrore.processor.rpc.proto.ListDestinationRequest;
import bifrore.processor.rpc.proto.ListDestinationResponse;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.core.Handler;
import io.vertx.core.json.JsonObject;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.GET;
import javax.ws.rs.Path;

@Path("/destination")
@Slf4j
public class ListDestinationHandler implements Handler<RoutingContext> {
    private final IProcessorClient processorClient;

    public ListDestinationHandler(IProcessorClient processorClient) {
        this.processorClient = processorClient;
    }

    @GET
    @Override
    public void handle(RoutingContext ctx) {
        processorClient.listDestination(ListDestinationRequest.newBuilder().build())
                .whenComplete((v, e) -> {
                   if (e != null) {
                       log.error("Failed to list destination", e);
                       ctx.response()
                               .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                               .end("List destination failed, error: " + e.getMessage());
                   } else if (v.getCode() != ListDestinationResponse.Code.OK) {
                       ctx.response()
                               .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                               .end("List destination failed, error: " + v.getReason());
                   }else {
                       ctx.response().
                               setStatusCode(HttpResponseStatus.OK.code())
                               .end(JsonObject.of("destinationIds", v.getDestinationIdsList()).encode());
                   }
                });
    }
}
