package bifromq.re.processor.server;

import bifromq.re.baserpc.RPCServerBuilder;
import bifromq.re.processor.worker.IProcessorWorker;
import bifromq.re.processor.worker.ProcessorWorkerBuilder;

public class ProcessorServerBuilder {
    IProcessorWorker processorWorker;
    RPCServerBuilder rpcServerBuilder;

    public ProcessorServerBuilder processorWorker(IProcessorWorker processorWorker) {
        this.processorWorker = processorWorker;
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
