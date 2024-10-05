package bifromq.re.admin.worker.handler;

import io.vertx.core.Handler;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.POST;
import javax.ws.rs.Path;

@Path("/update/ruleChain")
@Slf4j
public class UpdateRuleHandler implements Handler<RoutingContext> {

    public UpdateRuleHandler() {

    }

    @POST
    @Override
    public void handle(RoutingContext ctx) {

    }
}
