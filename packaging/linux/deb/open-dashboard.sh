#!/bin/sh
set -eu

exec "/opt/pole/pole-client" control-api-open "/etc/pole/node.json"
