package bifrore.common.parser.exception;

public class TopicFilterMissingException extends Exception {
    public TopicFilterMissingException() {
        super("topicFilter is missing");
    }
}
