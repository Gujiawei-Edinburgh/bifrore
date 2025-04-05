package bifrore.router.server;

class RouterServer implements IRouterServer{
    private final RouterService routerService;

    RouterServer(RouterServerBuilder serverBuilder) {
        routerService = new RouterService(serverBuilder.idMap,
                serverBuilder.topicFilterMap, serverBuilder.processorClient);
        serverBuilder.rpcServerBuilder.addService(routerService.bindService());
    }

    @Override
    public void start() {
        routerService.start();
    }

    @Override
    public void stop() {
        routerService.stop();
    }
}
