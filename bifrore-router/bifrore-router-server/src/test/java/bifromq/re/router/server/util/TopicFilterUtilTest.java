package bifromq.re.router.server.util;

import org.testng.annotations.Test;

import java.util.ArrayList;
import java.util.List;

public class TopicFilterUtilTest {
    private final String topic = "a/b/c";
    @Test
    public void testFullMatch() {
        String topicFilter = "a/b/c";
        assert TopicFilterUtil.isMatch(topic, topicFilter);
    }

    @Test
    public void testSingleLevelWildcardMatch() {
        List<String> topicFilters = new ArrayList<>() {{
            // single wildcard
            add("a/b/+");
            add("a/+/c");
            add("+/b/c");
            // 2 wildcards
            add("+/+/c");
            add("+/b/+");
            add("a/+/+");
            // 3 wildcards
            add("+/+/+");
        }};

        topicFilters.forEach(t -> {
            assert TopicFilterUtil.isMatch(topic, t);
        });
    }

    @Test
    public void testMultiLevelWildcardMatch() {
        List<String> topicFilters = new ArrayList<>() {{
            add("a/b/#");
            add("a/#");
            add("#");
        }};

        topicFilters.forEach(t -> {
            assert TopicFilterUtil.isMatch(topic, t);
        });
    }

    @Test
    public void testMixWildcardsMatch() {
        List<String> topicFilters = new ArrayList<>() {{
            add("a/+/#");
            add("+/b/#");
            add("+/+/#");
            add("+/b/+/#");
            add("a/+/+/#");
            add("a/b/+/#");
            add("a/b/c/#");
        }};

        topicFilters.forEach(t -> {
            assert TopicFilterUtil.isMatch(topic, t);
        });
    }

    @Test
    public void testNotMatch() {
        List<String> topicFilters = new ArrayList<>() {{
            add("a/b/d");
            add("a/+/c/d");
            add("a/c/#");
        }};

        topicFilters.forEach(t -> {
            assert !TopicFilterUtil.isMatch(topic, t);
        });
    }
}
