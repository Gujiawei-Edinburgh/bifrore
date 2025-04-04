package bifrore.admin.worker.handler;

import bifrore.baserpc.clock.HLC;
import bifrore.processor.client.IProcessorClient;
import bifrore.processor.rpc.proto.DeleteDestinationRequest;
import bifrore.processor.rpc.proto.DeleteDestinationResponse;
import io.netty.handler.codec.http.HttpResponseStatus;
import io.vertx.core.Handler;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.DELETE;
import javax.ws.rs.Path;

@Path("/destination")
@Slf4j
public class DeleteDestinationHandler implements Handler<RoutingContext> {
    private final IProcessorClient processorClient;

    public DeleteDestinationHandler(IProcessorClient processorClient) {
        this.processorClient = processorClient;
    }

    @DELETE
    @Override
    public void handle(RoutingContext ctx) {
        String destinationId = ctx.queryParams().get("destinationId");
        if (destinationId == null || destinationId.isEmpty()) {
            ctx.response().
                    setStatusCode(HttpResponseStatus.BAD_REQUEST.code())
                    .end("DestinationId is required");
            return;
        }
        DeleteDestinationRequest request = DeleteDestinationRequest.newBuilder()
                .setReqId(HLC.INST.get())
                .setDestinationId(destinationId)
                .build();
        processorClient.deleteDestination(request)
                .whenComplete((v, e) -> {
                    if (e != null) {
                        log.error("Failed to delete destination: {}", destinationId, e);
                        ctx.response()
                                .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                .end("Delete destination failed, error: " + e.getMessage());
                    } else if (v.getCode() != DeleteDestinationResponse.Code.OK) {
                        ctx.response()
                                .setStatusCode(HttpResponseStatus.INTERNAL_SERVER_ERROR.code())
                                .end("Delete destination failed, error: " + v.getReason());
                    }else {
                        ctx.response().
                                setStatusCode(HttpResponseStatus.OK.code())
                                .end("Delete destination ok");
                    }
                });
    }
}
