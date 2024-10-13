package bifromq.re.processor.server;


class ProcessorServer implements IProcessorServer {

    ProcessorServer(ProcessorServerBuilder builder) {
        ProcessorService processorService = new ProcessorService(builder.processorWorkerBuilder.build());
        builder.rpcServerBuilder.addService(processorService.bindService());
    }
}
