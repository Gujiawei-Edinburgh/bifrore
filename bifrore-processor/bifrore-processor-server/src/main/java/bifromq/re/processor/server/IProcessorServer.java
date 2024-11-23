package bifromq.re.processor.server;

import bifromq.re.processor.worker.ProcessorWorkerBuilder;

public interface IProcessorServer {
    static ProcessorServerBuilder newBuilder() {
        return new ProcessorServerBuilder();
    }
}
