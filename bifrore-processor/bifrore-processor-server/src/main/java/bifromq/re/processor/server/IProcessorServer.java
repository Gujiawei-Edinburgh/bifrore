package bifromq.re.processor.server;

public interface IProcessorServer {
    static ProcessorServerBuilder newBuilder() {
        return new ProcessorServerBuilder();
    }
}
