# MQTT-Based Rule Engine

# Overview

The MQTT-Based Rule Engine is a versatile solution designed for real-time rule evaluation and action execution. Built on the MQTT protocol, this engine connects to any MQTT broker, making it adaptable for diverse use cases.

# Key Features

1. MQTT Protocol Compatibility: The engine seamlessly connects to any MQTT broker, providing flexible integration options.
2. SQL Syntax for Rule Writing: Rules use familiar SQL syntax, with the "FROM" clause serving as a topic filter for intuitive rule evaluation.
3. Native Distributed Support: Built for distributed environments, the engine enables scalable rule processing across multiple nodes.
4. Plugin System for Integration: A flexible plugin system lets users integrate downstream services and develop custom actions for specific business needs.

# Architecture

![rule-engine.svg](docs/figures/rule-engine.svg)

## Core Components

### ProcessorWorker

The ProcessorWorker receives messages from MQTT brokers through shared subscription. It contains multiple MQTT clients to handle this task. When a message arrives, the worker uses the `Router` to find matching rules.

### Router

It matches rules based on the incoming message's topic.

### Processor

The Processor executes actions based on matched rules. It uses the rule's destination information to find the appropriate destination plugin for message delivery.

# QuickStart

- Start a MQTT broker

```bash
docker pull bifromq:latest
docker run --network host -d --name bifromq bifromq/bifromq:latest
```

- Start the BifroRE instance

```bash
./bin/standalone.sh start
```

- Add a `Destination`
```bash
curl -X PUT http://localhost:8088/destination \
     -H "Content-Type: application/json" \
     -d '{
           "destinationType": "kafka",
           "cfg": {
             "bootstrap.servers": "127.0.0.1:9092",
             "acks": "all",
           }
         }'
```

- Add A Rule and Test

```bash
 # Add a basic rule
 curl -X PUT http://localhost:8088/rule -d '{"expression": "select * from a", "destinations": ["DevOnly"]}'
 # Add a filtering and mapping rule
 curl -X PUT http://localhost:8088/rule -d '{"expression": "select 2*h as new_height, 2*w as new_width from \"a/b/c\" where temp > 25", "destinations": ["DevOnly"]}'
 # List the existing rules
 curl http://localhost:8088/rule
```

- Send a message on topic `a`

You can use any MQTT client tools (such as MQTTX) to send a message on the rule's topic. The rule engine will process 
the message based on the given rule and send the processed messages to the destinations.

- Delete a Rule
```bash
curl -X DELETE "http://localhost:8088/rule?ruleId=$YOUR_RULE_ID"
```
The corresponding rule will be deleted. If all the rules are deleted for a given topicFilter, the rule engine will 
unsubscribe the topicFilter.
