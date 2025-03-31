package bifrore.admin.worker.http;

import java.util.Map;

public record AddDestinationHttpRequest(String destinationType, Map<String, String> cfg) {
}
