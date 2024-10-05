package bifromq.re.baserpc.util;

import io.netty.channel.EventLoopGroup;
import io.netty.channel.epoll.Epoll;
import io.netty.channel.epoll.EpollEventLoopGroup;
import io.netty.channel.epoll.EpollServerSocketChannel;
import io.netty.channel.epoll.EpollSocketChannel;
import io.netty.channel.nio.NioEventLoopGroup;
import io.netty.channel.socket.ServerSocketChannel;
import io.netty.channel.socket.SocketChannel;
import io.netty.channel.socket.nio.NioServerSocketChannel;
import io.netty.channel.socket.nio.NioSocketChannel;
import java.util.concurrent.ThreadFactory;
import lombok.extern.slf4j.Slf4j;


@Slf4j
public class NettyUtil {
    public static EventLoopGroup createEventLoopGroup() {
        if (Epoll.isAvailable()) {
            return new EpollEventLoopGroup();
        }
        return new NioEventLoopGroup();
    }

    public static EventLoopGroup createEventLoopGroup(int nThreads) {
        return createEventLoopGroup(nThreads, null);
    }

    public static EventLoopGroup createEventLoopGroup(int nThreads, ThreadFactory threadFactory) {
        if (Epoll.isAvailable()) {
            return new EpollEventLoopGroup(nThreads, threadFactory);
        }
        return new NioEventLoopGroup(nThreads, threadFactory);

    }

    public static Class<? extends SocketChannel> getSocketChannelClass() {
        if (Epoll.isAvailable()) {
            log.debug("Epoll is available on this platform");
            return EpollSocketChannel.class;
        }
        log.debug("Neither Epoll nor KQueue is available on this platform");
        return NioSocketChannel.class;
    }

    public static Class<? extends ServerSocketChannel> getServerSocketChannelClass() {
        if (Epoll.isAvailable()) {
            log.debug("Epoll is available on this platform");
            return EpollServerSocketChannel.class;
        }
        log.debug("Neither Epoll nor KQueue is available on this platform");
        return NioServerSocketChannel.class;
    }

    public static Class<? extends SocketChannel> determineSocketChannelClass(EventLoopGroup eventLoopGroup) {
        if (eventLoopGroup instanceof EpollEventLoopGroup) {
            return EpollSocketChannel.class;
        }
        return NioSocketChannel.class;
    }

    public static Class<? extends ServerSocketChannel> determineServerSocketChannelClass(
        EventLoopGroup eventLoopGroup) {
        if (eventLoopGroup instanceof EpollEventLoopGroup) {
            return EpollServerSocketChannel.class;
        }
        return NioServerSocketChannel.class;
    }
}
