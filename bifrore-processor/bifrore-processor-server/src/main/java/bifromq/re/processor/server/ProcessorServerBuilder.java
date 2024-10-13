package bifromq.re.processor.server;

import bifromq.re.baserpc.RPCServerBuilder;
import bifromq.re.processor.worker.ProcessorWorkerBuilder;

public class ProcessorServerBuilder {
    ProcessorWorkerBuilder processorWorkerBuilder;
    RPCServerBuilder rpcServerBuilder;

    public ProcessorWorkerBuilder processorWorkerBuilder(ProcessorWorkerBuilder processorWorkerBuilder) {
        this.processorWorkerBuilder = processorWorkerBuilder;
        return processorWorkerBuilder;
    }

    public RPCServerBuilder rpcServerBuilder(RPCServerBuilder rpcServerBuilder) {
        this.rpcServerBuilder = rpcServerBuilder;
        return rpcServerBuilder;
    }

    public IProcessorServer build() {
        return new ProcessorServer(this);
    }
}
