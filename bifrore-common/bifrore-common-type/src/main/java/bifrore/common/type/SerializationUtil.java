package bifrore.common.type;

import bifrore.commontype.ListMessage;
import bifrore.commontype.MapMessage;
import com.google.protobuf.ByteString;
import com.google.protobuf.InvalidProtocolBufferException;

import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

public class SerializationUtil {
    public static byte[] serializeMap(Map<String, String> mapMessage) {
        return MapMessage.newBuilder().putAllMapMessage(mapMessage).build().toByteArray();
    }

    public static Map<String, String> deserializeMap(byte[] bytes) throws InvalidProtocolBufferException {
        MapMessage mapMessage = MapMessage.parseFrom(bytes);
        return mapMessage.getMapMessageMap();
    }

    public static byte[] serializeList(List<byte[]> list) {
        ListMessage.Builder builder = ListMessage.newBuilder();
        for (byte[] bytes : list) {
            builder.addElement(ByteString.copyFrom(bytes));
        }
        return builder.build().toByteArray();
    }

    public static List<byte[]> deserializeList(byte[] data) {
       try {
           ListMessage byteArrayList = ListMessage.parseFrom(data);
           return byteArrayList.getElementList().stream()
                   .map(ByteString::toByteArray)
                   .collect(Collectors.toList());
       }catch (InvalidProtocolBufferException e){
           throw new RuntimeException(e);
       }
    }
}
