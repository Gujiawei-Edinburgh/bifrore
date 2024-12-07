package bifrore.baserpc;

public interface IRPCServer {
    static RPCServerBuilder newBuilder() {
        return new RPCServerBuilder();
    }

    String id();

    void start();

    void shutdown();
}
