package bifrore.common.type;

import bifrore.commontype.MapMessage;
import com.google.protobuf.InvalidProtocolBufferException;

import java.util.Map;

public class MapMessageUtil {
    public static byte[] serialize(Map<String, String> mapMessage) {
        return MapMessage.newBuilder().putAllMapMessage(mapMessage).build().toByteArray();
    }

    public static Map<String, String> deserialize(byte[] bytes) throws InvalidProtocolBufferException {
        MapMessage mapMessage = MapMessage.parseFrom(bytes);
        return mapMessage.getMapMessageMap();
    }
}
