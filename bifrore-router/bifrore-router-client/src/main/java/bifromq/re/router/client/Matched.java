package bifromq.re.router.client;

import bifromq.re.common.parser.Parsed;

import java.util.List;

public record Matched(Parsed parsed, List<String> destinations) {
}
