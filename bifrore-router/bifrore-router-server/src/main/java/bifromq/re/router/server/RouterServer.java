package bifromq.re.router.server;

class RouterServer implements IRouterServer{

    RouterServer(RouterServerBuilder serverBuilder) {
        RouterService routerService = new RouterService(serverBuilder.idMap, serverBuilder.topicFilterMap);
        serverBuilder.rpcServerBuilder.addService(routerService.bindService());
    }
}
