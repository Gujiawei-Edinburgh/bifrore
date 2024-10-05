package bifromq.re.common.parser.exception;

public class TopicFilterMissingException extends Exception {
    public TopicFilterMissingException() {
        super("topicFilter is missing");
    }
}
