package bifromq.re.common.parser.util;

import bifromq.re.common.parser.Parsed;

import java.io.*;

public class SerializeUtil {
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
