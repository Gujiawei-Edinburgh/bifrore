package bifromq.re.baserpc.nameresolver;

import bifromq.re.baserpc.discovery.ITrafficDiscovery;
import io.grpc.Attributes;
import io.grpc.EquivalentAddressGroup;
import io.grpc.NameResolver;
import io.grpc.Status;
import io.reactivex.rxjava3.disposables.CompositeDisposable;
import lombok.extern.slf4j.Slf4j;

import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;

@Slf4j
public class ServiceNameResolver extends NameResolver {
    private final String serviceUniqueName;
    private final ITrafficDiscovery trafficDiscovery;
    private final CompositeDisposable disposable = new CompositeDisposable();

    public ServiceNameResolver(String serviceUniqueName) {
        this.serviceUniqueName = serviceUniqueName;
        this.trafficDiscovery = ITrafficDiscovery.getInstance(serviceUniqueName);
    }

    @Override
    public String getServiceAuthority() {
        return serviceUniqueName;
    }

    @Override
    public void start(Listener listener) {
        log.info("Starting NameResolver for service[{}]", serviceUniqueName);
        disposable.add(trafficDiscovery.start().subscribe(
                servers -> listener.onAddresses(toAddressGroup(servers), toAttributes()),
                throwable -> listener.onError(Status.INTERNAL.withCause(throwable))
        ));
    }

    @Override
    public void shutdown() {
        log.info("Start to shutdown trafficGovernor nameResolver, service={}", serviceUniqueName);
        disposable.dispose();
        trafficDiscovery.stop();
    }

    private List<EquivalentAddressGroup> toAddressGroup(Set<ITrafficDiscovery.Server> servers) {
        return servers.stream()
                .map(s -> new EquivalentAddressGroup(s.hostAddr, Attributes.EMPTY))
                .collect(Collectors.toList());
    }

    private Attributes toAttributes() {
        return Attributes.newBuilder().build();
    }
}
