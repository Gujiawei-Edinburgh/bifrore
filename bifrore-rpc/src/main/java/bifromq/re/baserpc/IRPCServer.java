package bifromq.re.baserpc;

public interface IRPCServer {
    static RPCServerBuilder newBuilder() {
        return new RPCServerBuilder();
    }

    String id();

    void start();

    void shutdown();
}
