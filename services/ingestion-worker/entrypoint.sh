#!/bin/sh
set -e

echo "Ingestion Worker entrypoint starting"
echo "PWD: $(pwd)"
ls -l /root || true

if [ ! -x /root/ingestion-worker ]; then
  echo "Binary not executable or missing: /root/ingestion-worker"
  ls -l /root/ingestion-worker || true
fi

echo "API_GATEWAY_URL set: ${API_GATEWAY_URL:+yes}${API_GATEWAY_URL:-no}"
echo "REDIS_URL set: ${REDIS_URL:+yes}${REDIS_URL:-no}"
echo "NEO4J_URI set: ${NEO4J_URI:+yes}${NEO4J_URI:-no}"

echo "Binary info:"
file /root/ingestion-worker || true
ldd /root/ingestion-worker || true

export RUST_BACKTRACE=1

set +e
/root/ingestion-worker
code=$?
set -e

echo "ingestion-worker exited with code ${code}"
exit $code
