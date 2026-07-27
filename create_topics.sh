#!/usr/bin/env bash
# Creates the Kafka topics the services rely on. Idempotent (--if-not-exists),
# so it is safe to re-run. Invoked by the kafka-init service once the broker is
# healthy.
set -euo pipefail

BOOTSTRAP="${KAFKA_BOOTSTRAP_SERVER:-broker:9092}"

# topic name : partitions : replication-factor
TOPICS=(
  "auth.email.verification:3:1"   # confirmo-auth -> confirmo-notification-service
  "core.email.verified:3:1"       # confirmo-auth -> confirmo-graphql
)

for entry in "${TOPICS[@]}"; do
  IFS=":" read -r topic partitions rf <<<"$entry"
  echo "Ensuring topic '${topic}' (partitions=${partitions}, rf=${rf})"
  /opt/kafka/bin/kafka-topics.sh \
    --bootstrap-server "$BOOTSTRAP" \
    --create --if-not-exists \
    --topic "$topic" \
    --partitions "$partitions" \
    --replication-factor "$rf"
done

echo "All topics ready."
