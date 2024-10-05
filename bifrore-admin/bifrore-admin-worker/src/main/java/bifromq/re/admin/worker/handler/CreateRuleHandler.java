package bifromq.re.admin.worker.handler;

import io.vertx.core.Handler;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.PUT;
import javax.ws.rs.Path;

@Path("/chain/create")
@Slf4j
public class CreateRuleHandler implements Handler<RoutingContext> {

    public CreateRuleHandler() {

    }

    @PUT
    @Override
    public void handle(RoutingContext ctx) {

    }
}
