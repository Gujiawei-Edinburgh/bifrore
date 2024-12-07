package bifrore.processor.server;

import bifrore.baserpc.RPCServerBuilder;
import bifrore.processor.worker.IProcessorWorker;

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
