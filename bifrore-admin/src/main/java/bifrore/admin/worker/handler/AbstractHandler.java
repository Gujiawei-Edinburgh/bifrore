package bifrore.admin.worker.handler;

import bifrore.router.client.IRouterClient;
import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.hubspot.jackson.datatype.protobuf.ProtobufModule;
import io.vertx.core.Handler;
import io.vertx.ext.web.RoutingContext;

import java.util.Optional;

abstract class AbstractHandler implements Handler<RoutingContext> {
    final IRouterClient routerClient;
    final ObjectMapper objectMapper = new ObjectMapper().registerModule(new ProtobufModule());

    public AbstractHandler(IRouterClient routerClient) {
        this.routerClient = routerClient;
    }

    public <Resp> Optional<String> buildJsonSting(Resp response) {
        try {
            String jsonResponse = objectMapper.writeValueAsString(response);
            return Optional.of(jsonResponse);
        }catch (JsonProcessingException e) {
            return Optional.empty();
        }
    }
}
