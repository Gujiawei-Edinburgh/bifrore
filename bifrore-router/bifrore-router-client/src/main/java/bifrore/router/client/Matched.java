package bifrore.router.client;

import bifrore.common.parser.Parsed;

import java.util.List;

public record Matched(Parsed parsed, List<String> destinations) {
}
