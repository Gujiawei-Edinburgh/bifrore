package bifrore.admin.worker.http;

import bifrore.processor.rpc.proto.DestinationMeta;

import java.util.List;

public record ListDestinationHttpResponse(List<DestinationMeta> destinationMetaList) {
}
