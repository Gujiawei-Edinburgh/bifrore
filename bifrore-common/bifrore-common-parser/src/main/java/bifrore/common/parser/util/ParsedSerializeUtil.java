package bifrore.common.parser.util;

import bifrore.common.parser.Parsed;

import java.io.*;

public class ParsedSerializeUtil {
    public static byte[] serializeParsed(Parsed parsed) throws IOException {
        ByteArrayOutputStream byteArrayOutputStream = new ByteArrayOutputStream();
        ObjectOutputStream objectOutputStream = new ObjectOutputStream(byteArrayOutputStream);
        objectOutputStream.writeObject(parsed);
        objectOutputStream.flush();
        objectOutputStream.close();
        return byteArrayOutputStream.toByteArray();
    }

    public static Parsed deserializeParsed(byte[] bytes) throws IOException, ClassNotFoundException {
        ByteArrayInputStream byteArrayInputStream = new ByteArrayInputStream(bytes);
        ObjectInputStream objectInputStream = new ObjectInputStream(byteArrayInputStream);
        Parsed parsedObj = (Parsed) objectInputStream.readObject();
        objectInputStream.close();
        return parsedObj;
    }
}
