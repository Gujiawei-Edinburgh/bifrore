package bifrore.processor.server;


class ProcessorServer implements IProcessorServer {
    private final ProcessorService processorService;
    ProcessorServer(ProcessorServerBuilder builder) {
        processorService = new ProcessorService(builder.processorWorker);
        builder.rpcServerBuilder.addService(processorService.bindService());
    }

    @Override
    public void start() {
        processorService.start();
    }

    @Override
    public void stop() {
        processorService.stop();
    }
}
