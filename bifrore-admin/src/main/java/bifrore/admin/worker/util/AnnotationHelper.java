package bifrore.admin.worker.util;

import io.netty.handler.codec.http.HttpMethod;
import io.vertx.core.Handler;
import io.vertx.ext.web.RoutingContext;

import javax.ws.rs.DELETE;
import javax.ws.rs.GET;
import javax.ws.rs.HEAD;
import javax.ws.rs.OPTIONS;
import javax.ws.rs.PATCH;
import javax.ws.rs.POST;
import javax.ws.rs.PUT;
import javax.ws.rs.Path;
import java.lang.reflect.Method;

public class AnnotationHelper {
    public static <T extends Handler<RoutingContext>> HttpMethod getHTTPMethod(Class<T> handlerClass) {
        Method handleMethod = handlerClass.getMethods()[0];
        assert handleMethod.getName().equals("handle");
        if (handleMethod.getAnnotation(GET.class) != null) {
            return HttpMethod.GET;
        } else if (handleMethod.getAnnotation(PUT.class) != null) {
            return HttpMethod.PUT;
        } else if (handleMethod.getAnnotation(POST.class) != null) {
            return HttpMethod.POST;
        } else if (handleMethod.getAnnotation(DELETE.class) != null) {
            return HttpMethod.DELETE;
        } else if (handleMethod.getAnnotation(OPTIONS.class) != null) {
            return HttpMethod.OPTIONS;
        } else if (handleMethod.getAnnotation(HEAD.class) != null) {
            return HttpMethod.HEAD;
        } else if (handleMethod.getAnnotation(PATCH.class) != null) {
            return HttpMethod.PATCH;
        }
        return null;
    }

    public static <T extends Handler<RoutingContext>> Path getPath(Class<T> handlerClass) {
        return handlerClass.getAnnotation(Path.class);
    }
}
