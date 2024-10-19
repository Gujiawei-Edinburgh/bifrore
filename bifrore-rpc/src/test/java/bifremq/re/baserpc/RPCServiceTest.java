package bifremq.re.baserpc;

import bifromq.re.baserpc.IClusterManager;
import bifromq.re.baserpc.IRPCClient;
import bifromq.re.baserpc.IRPCServer;
import bifromq.re.baserpc.test.RPCTestGrpc;
import bifromq.re.baserpc.test.Request;
import bifromq.re.baserpc.test.Response;
import bifromq.re.baserpc.util.NettyUtil;
import com.google.common.util.concurrent.MoreExecutors;
import com.google.protobuf.ByteString;
import com.hazelcast.core.Hazelcast;
import io.grpc.stub.StreamObserver;
import org.testng.annotations.AfterTest;
import org.testng.annotations.BeforeTest;
import org.testng.annotations.Test;
import java.util.HashMap;
import java.util.Map;

public class RPCServiceTest {
    private final IClusterManager clusterManager = IClusterManager.newBuilder()
            .servers(Hazelcast.newHazelcastInstance().getSet("servers"))
            .build();
    private final IRPCServer server = IRPCServer.newBuilder()
            .id("testId")
            .host("127.0.0.1")
            .port(8080)
            .sslContext(null)
            .bossEventLoopGroup(NettyUtil.createEventLoopGroup())
            .workerEventLoopGroup(NettyUtil.createEventLoopGroup())
            .addService(new RPCTestGrpc.RPCTestImplBase() {
                @Override
                public void unary(Request request, StreamObserver<Response> responseObserver) {
                    responseObserver.onNext(Response.newBuilder()
                            .setId(request.getId())
                            .setValue("testValue")
                            .setBin(ByteString.copyFromUtf8("testBin"))
                            .build());
                    responseObserver.onCompleted();
                }
            }.bindService())
            .clusterManager(clusterManager)
            .build();
    private final IRPCClient client = IRPCClient.newBuilder()
            .serviceUniqueName("test.RPCTest")
            .sslContext(null)
            .eventLoopGroup(NettyUtil.createEventLoopGroup())
            .executor(MoreExecutors.directExecutor())
            .clusterManager(clusterManager)
            .build();
    private final Map<String, String> metadata = new HashMap<>();

    @BeforeTest
    public void setUp() {
        server.start();
    }

    @AfterTest
    public void tearDown() {
        server.shutdown();
        clusterManager.close();
    }

    @Test
    public void testUnaryCall() {
        Request request = Request.newBuilder()
                .setId(1)
                .setValue("testValue")
                .setBin(ByteString.copyFromUtf8("testBin"))
                .build();
        Response response = client.invoke(request, metadata, RPCTestGrpc.getUnaryMethod()).join();
        assert response.getId() == request.getId();
        assert response.getValue().equals("testValue");
        assert "testBin".equals(response.getBin().toStringUtf8());
    }

}
