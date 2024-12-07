package bifrore.baserpc.nameresolver;

import bifrore.baserpc.IClusterManager;
import io.grpc.NameResolver;
import io.grpc.NameResolverProvider;

import java.net.URI;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

public class ServiceNameResolverProvider extends NameResolverProvider {
    public static final String SCHEME = "tdsc";
    public static NameResolverProvider INSTANCE = new ServiceNameResolverProvider();
    private static final Map<String, ServiceNameResolver> RESOLVERS = new ConcurrentHashMap<>();

    @Override
    protected boolean isAvailable() {
        return true;
    }

    @Override
    protected int priority() {
        return Integer.MAX_VALUE;
    }

    public static void register(String serviceUniqueName, IClusterManager clusterManager) {
        RESOLVERS.put(serviceUniqueName, new ServiceNameResolver(serviceUniqueName, clusterManager));
    }

    @Override
    public NameResolver newNameResolver(URI targetUri, NameResolver.Args args) {
        if (SCHEME.equals(targetUri.getScheme())) {
            return RESOLVERS.get(targetUri.getAuthority());
        }
        return null;
    }

    @Override
    public String getDefaultScheme() {
        return SCHEME;
    }
}
