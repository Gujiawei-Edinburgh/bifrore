package bifromq.re.processor.worker;

import bifromq.re.commontype.Message;

public interface IProducer {
    IProducer DUMMY = new IProducer() {
        @Override
        public void send(Message message) {}

        @Override
        public void close() {

        }
    };
    void send(Message message);

    void close();
}
