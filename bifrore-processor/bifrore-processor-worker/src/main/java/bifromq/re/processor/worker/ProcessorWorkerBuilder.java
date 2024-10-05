package bifromq.re.processor.worker;

public class ProcessorWorkerBuilder {
    int clientNum;
    String userName;
    String password;
    boolean cleanStart;
    boolean ordered;
    long sessionExpiryInterval;
    String host;
    int port;
    String clientPrefix;
    IProducer producer;

    public ProcessorWorkerBuilder clientNum(int clientNum) {
        this.clientNum = clientNum;
        return this;
    }

    public ProcessorWorkerBuilder userName(String userName) {
        this.userName = userName;
        return this;
    }

    public ProcessorWorkerBuilder password(String password) {
        this.password = password;
        return this;
    }

    public ProcessorWorkerBuilder cleanStart(boolean cleanStart) {
        this.cleanStart = cleanStart;
        return this;
    }

    public ProcessorWorkerBuilder ordered(boolean ordered) {
        this.ordered = ordered;
        return this;
    }

    public ProcessorWorkerBuilder sessionExpiryInterval(long sessionExpiryInterval) {
        this.sessionExpiryInterval = sessionExpiryInterval;
        return this;
    }

    public ProcessorWorkerBuilder host(String host) {
        this.host = host;
        return this;
    }

    public ProcessorWorkerBuilder port(int port) {
        this.port = port;
        return this;
    }

    public ProcessorWorkerBuilder clientPrefix(String clientPrefix) {
        this.clientPrefix = clientPrefix;
        return this;
    }

    public ProcessorWorkerBuilder producer(IProducer producer) {
        this.producer = producer;
        return this;
    }

    public ProcessorWorker build() {
        return new ProcessorWorker(this);
    }
}
