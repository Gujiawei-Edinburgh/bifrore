package bifromq.re.admin.worker;

public interface IAdminServer {
    static AdminServerBuilder newBuilder() {
        return new AdminServerBuilder();
    }
}
