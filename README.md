# MQTT-Based Rule Engine

# Overview

The MQTT-Based Rule Engine is a versatile solution designed for real-time rule evaluation and action execution. Built on the MQTT protocol, this engine connects to any MQTT broker, making it adaptable for diverse use cases.

# Key Features

1. MQTT Protocol Compatibility: The engine seamlessly connects to any MQTT broker, providing flexible integration options.
2. SQL Syntax for Rule Writing: Rules use familiar SQL syntax, with the "FROM" clause serving as a topic filter for intuitive rule evaluation.
3. Native Distributed Support: Built for distributed environments, the engine enables scalable rule processing across multiple nodes.
4. Plugin System for Integration: A flexible plugin system lets users integrate downstream services and develop custom actions for specific business needs.

# Architecture

![rule-engine.svg](./readme/rule-engine.svg)

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
docker pull bifrore:latest
docker run --network host -d --name bifrore bifrore:latest
```

- Add A Rule and Test

```bash
 # Add a rule
 curl -X PUT http://localhost/add/rule -d '{"expression": "select * from a", "destinations": ["DevOnly"]}'
 # List the existing rules
 curl http://localhost/list/rule
```

- Send a message on topic `a`

You can use any MQTT UI tool (such as MQTTX) to send a message on the rule's topic. The rule engine will process the message using the default DevOnly destination plugin.