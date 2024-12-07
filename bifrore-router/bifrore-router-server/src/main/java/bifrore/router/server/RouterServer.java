package bifrore.router.server;

class RouterServer implements IRouterServer{

    RouterServer(RouterServerBuilder serverBuilder) {
        RouterService routerService = new RouterService(serverBuilder.idMap,
                serverBuilder.topicFilterMap, serverBuilder.processorClient);
        serverBuilder.rpcServerBuilder.addService(routerService.bindService());
    }
}
