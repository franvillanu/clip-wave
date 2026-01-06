#!/usr/bin/env bash
set -euo pipefail

ROLE_NAME="${1:-}"
TASK_FILE="${2:-}"

if [[ -z "$ROLE_NAME" || -z "$TASK_FILE" ]]; then
  echo "Usage: ./.sdd/sdd.sh role <role> <task-file>"
  echo "Example: ./.sdd/sdd.sh role architect .sdd/tasks/feature-x.md"
  exit 1
fi

ROLE_FILE=".sdd/roles/${ROLE_NAME}.md"

if [[ ! -f "$ROLE_FILE" ]]; then
  echo "Role file not found: $ROLE_FILE"
  exit 1
fi

if [[ ! -f "$TASK_FILE" ]]; then
  echo "Task file not found: $TASK_FILE"
  exit 1
fi

echo "=== ROLE ==="
cat "$ROLE_FILE"
echo
echo "=== TASK ==="
cat "$TASK_FILE"
echo
echo "=== INSTRUCTION ==="
echo "Follow the role instructions and produce output in the role's required format."