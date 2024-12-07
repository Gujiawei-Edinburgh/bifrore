#!/bin/bash

# fork from BifroMQ

BASE_DIR=$(
  cd "$(dirname "$0")"
  pwd
)/..
export LOG_DIR=$BASE_DIR/logs

if [ $# -lt 1 ]; then
  echo "USAGE: $0 {start|stop|restart} [-fg]"
  exit 1
fi

COMMAND=$1
shift

if [ $COMMAND = "start" ]; then
  exec "$BASE_DIR/bin/bifrore-start.sh" -c bifrore.starter.StandaloneStarter -f standalone.yml "$@"
elif [ $COMMAND = "stop" ]; then
  exec "$BASE_DIR/bin/bifrore-stop.sh" StandaloneStarter
elif [ $COMMAND = "restart" ]; then
  sh "$BASE_DIR/bin/bifrore-stop.sh" StandaloneStarter
  "$BASE_DIR/bin/bifrore-start.sh" -c bifrore.starter.StandaloneStarter -f standalone.yml "$@"
fi
