package bifrore.admin.worker;

import bifrore.processor.client.IProcessorClient;
import bifrore.router.client.IRouterClient;
import io.vertx.core.http.HttpServerRequest;
import io.vertx.core.http.HttpServerResponse;
import io.vertx.ext.web.RoutingContext;
import lombok.SneakyThrows;
import org.mockito.Mock;
import org.mockito.MockitoAnnotations;
import org.testng.annotations.AfterMethod;
import org.testng.annotations.BeforeMethod;


import static org.mockito.ArgumentMatchers.anyInt;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.Mockito.when;

public abstract class MockableTest {
    AutoCloseable closeable;
    @Mock
    protected IRouterClient routerClient;
    @Mock
    protected IProcessorClient processorClient;
    @Mock
    protected RoutingContext ctx;
    @Mock
    protected HttpServerRequest request;
    @Mock
    protected HttpServerResponse response;

    @BeforeMethod
    public void setup() {
        closeable = MockitoAnnotations.openMocks(this);
        when(ctx.request()).thenReturn(request);
        when(ctx.response()).thenReturn(response);
        when(response.setStatusCode(anyInt())).thenReturn(response);
        when(response.end(anyString())).thenReturn(null);
    }

    @SneakyThrows
    @AfterMethod
    public void teardown() {
        closeable.close();
    }
}
