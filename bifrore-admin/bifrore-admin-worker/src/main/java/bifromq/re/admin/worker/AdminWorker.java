package bifromq.re.admin.worker;

import bifromq.re.admin.worker.util.AnnotationHelper;
import io.netty.handler.codec.http.HttpMethod;
import io.vertx.core.Handler;
import io.vertx.core.Vertx;
import io.vertx.ext.web.Router;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.Path;
import java.util.List;

@Slf4j
public class AdminWorker {
    private final Router router;

    public AdminWorker(Vertx vertx, int port, List<Handler<RoutingContext>> handlers) {
        this.router = Router.router(vertx);
        handlers.forEach(this::addToRouter);
        vertx.createHttpServer().listen(port, asyncResult -> {
            if (asyncResult.succeeded()) {
                log.info("bifrore admin worker started on: {}", port);
            } else {
                log.error("Failed to start HTTP server: {}", asyncResult.cause().getMessage());
            }
        });
    }

    private void addToRouter(Handler<RoutingContext> handler) {
        HttpMethod method = AnnotationHelper.getHTTPMethod(handler.getClass());
        Path path = AnnotationHelper.getPath(handler.getClass());
        if (method != null && path != null) {
            if (method.equals(HttpMethod.GET)) {
                router.get(path.value()).handler(handler);
            } else if (method.equals(HttpMethod.PUT)) {
                router.put(path.value()).handler(handler);
            } else if (method.equals(HttpMethod.POST)) {
                router.post(path.value()).handler(handler);
            } else if (method.equals(HttpMethod.DELETE)) {
                router.delete(path.value()).handler(handler);
            } else if (method.equals(HttpMethod.OPTIONS)) {
                router.options(path.value()).handler(handler);
            } else if (method.equals(HttpMethod.HEAD)) {
                router.head(path.value()).handler(handler);
            }else if (method.equals(HttpMethod.PATCH)) {
                router.patch(path.value()).handler(handler);
            }else {
                log.warn("unsupported HTTP method: {}", method);
            }
        }else {
            log.warn("No http method specified for http request handler: {}", handler.getClass().getName());
        }
    }
}
