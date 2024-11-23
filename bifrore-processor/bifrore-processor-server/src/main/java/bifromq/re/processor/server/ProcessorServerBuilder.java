package bifromq.re.processor.server;

import bifromq.re.baserpc.RPCServerBuilder;
import bifromq.re.processor.worker.ProcessorWorkerBuilder;

public class ProcessorServerBuilder {
    ProcessorWorkerBuilder processorWorkerBuilder;
    RPCServerBuilder rpcServerBuilder;

    public ProcessorServerBuilder processorWorkerBuilder(ProcessorWorkerBuilder processorWorkerBuilder) {
        this.processorWorkerBuilder = processorWorkerBuilder;
        return this;
    }

    public ProcessorServerBuilder rpcServerBuilder(RPCServerBuilder rpcServerBuilder) {
        this.rpcServerBuilder = rpcServerBuilder;
        return this;
    }

    public IProcessorServer build() {
        return new ProcessorServer(this);
    }
}
