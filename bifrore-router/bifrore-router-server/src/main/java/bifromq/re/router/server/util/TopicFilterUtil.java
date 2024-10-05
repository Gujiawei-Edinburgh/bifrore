package bifromq.re.router.server.util;

public class TopicFilterUtil {
    public static boolean isMatch(String topic, String topicFilter) {
        String[] topicLevels = topic.split("/");
        String[] filterLevels = topicFilter.split("/");

        int topicIndex = 0;
        int filterIndex = 0;

        while (filterIndex < filterLevels.length) {
            String filterPart = filterLevels[filterIndex];

            if (filterPart.equals("#")) {
                return true;
            }

            if (topicIndex >= topicLevels.length) {
                return false;
            }

            String topicPart = topicLevels[topicIndex];

            if (!filterPart.equals("+") && !filterPart.equals(topicPart)) {
                return false;
            }

            topicIndex++;
            filterIndex++;
        }

        return topicIndex == topicLevels.length && filterIndex == filterLevels.length;
    }
}
