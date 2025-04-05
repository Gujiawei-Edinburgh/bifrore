package bifrore.processor.server;

public interface IProcessorServer {
    static ProcessorServerBuilder newBuilder() {
        return new ProcessorServerBuilder();
    }

    void start();

    void stop();
}
