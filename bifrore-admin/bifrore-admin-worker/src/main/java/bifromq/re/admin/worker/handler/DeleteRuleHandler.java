package bifromq.re.admin.worker.handler;

import bifromq.re.admin.worker.http.DeleteRuleRequest;
import io.vertx.core.Handler;
import io.vertx.core.json.Json;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.DELETE;
import javax.ws.rs.Path;

@Path("/chain/delete")
@Slf4j
public class DeleteRuleHandler implements Handler<RoutingContext> {

    public DeleteRuleHandler() {

    }

    @DELETE
    @Override
    public void handle(RoutingContext ctx) {
        DeleteRuleRequest request = Json.decodeValue(ctx.body().asString(), DeleteRuleRequest.class);
    }
}
