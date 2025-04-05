package bifrore.router.server;

public interface IRouterServer {
    static RouterServerBuilder newBuilder() {
        return new RouterServerBuilder();
    }

    void start();

    void stop();
}
