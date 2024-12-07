package bifrore.processor.server;


class ProcessorServer implements IProcessorServer {

    ProcessorServer(ProcessorServerBuilder builder) {
        ProcessorService processorService = new ProcessorService(builder.processorWorker);
        builder.rpcServerBuilder.addService(processorService.bindService());
    }
}
