package bifromq.re.router.server;

public interface IRouterServer {
    static RouterServerBuilder newBuilder() {
        return new RouterServerBuilder();
    }
}
