package bifromq.re.admin.worker;

import io.vertx.core.Handler;
import io.vertx.core.Vertx;
import io.vertx.ext.web.RoutingContext;

import java.util.ArrayList;
import java.util.List;

public class AdminWorkerBuilder {
    private Vertx vertx;
    private int port;
    private final List<Handler<RoutingContext>> handlers = new ArrayList<>();

    public AdminWorkerBuilder vertx(Vertx vertx) {
        this.vertx = vertx;
        return this;
    }

    public AdminWorkerBuilder port(int port) {
        this.port = port;
        return this;
    }

    public AdminWorkerBuilder addHandler(Handler<RoutingContext> handler) {
        this.handlers.add(handler);
        return this;
    }

    public AdminWorkerBuilder addHandlers(List<Handler<RoutingContext>> handlers) {
        this.handlers.addAll(handlers);
        return this;
    }

    public AdminWorker build() {
        if (vertx == null) {
            throw new IllegalArgumentException("Vertx instance must be provided");
        }
        if (port <= 0) {
            throw new IllegalArgumentException("Port must be a positive number");
        }
        if (handlers.isEmpty()) {
            throw new IllegalArgumentException("At least one handler must be added");
        }
        return new AdminWorker(vertx, port, handlers);
    }
}
