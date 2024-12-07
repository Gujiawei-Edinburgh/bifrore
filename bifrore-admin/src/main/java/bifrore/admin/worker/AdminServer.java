package bifrore.admin.worker;

import bifrore.admin.worker.util.AnnotationHelper;
import io.netty.handler.codec.http.HttpMethod;
import io.vertx.core.Handler;
import io.vertx.core.Vertx;
import io.vertx.core.http.HttpServer;
import io.vertx.ext.web.Router;
import io.vertx.ext.web.RoutingContext;
import lombok.extern.slf4j.Slf4j;

import javax.ws.rs.Path;

@Slf4j
class AdminServer implements IAdminServer {
    private final int port;
    private final Router router;

    public AdminServer(AdminServerBuilder builder) {
        Vertx vertx = builder.vertx;
        router = Router.router(builder.vertx);
        port = builder.port;
        builder.handlers.forEach(this::addToRouter);
        HttpServer server = vertx.createHttpServer();
        server.requestHandler(router).listen(builder.port, asyncResult -> {
            if (asyncResult.succeeded()) {
                log.info("Bifrore admin worker started on: {}", port);
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
                log.warn("Unsupported HTTP method: {}", method);
            }
        }else {
            log.warn("No http method specified for http request handler: {}", handler.getClass().getName());
        }
    }
}
