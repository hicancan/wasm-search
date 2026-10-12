#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 5 ]; then
  echo "::error::usage: $0 <dist-dir-or-empty> <project> <environment> <area> <result-path>"
  exit 2
fi

dist_dir="$1"
project="$2"
environment="$3"
area="$4"
result_path="$5"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if [ -z "${EDGEONE_API_TOKEN:-}" ]; then
  echo "::error::EDGEONE_API_TOKEN secret is required"
  exit 1
fi

if [ -n "$dist_dir" ] && [ ! -d "$dist_dir" ]; then
  echo "::error::EdgeOne deploy directory does not exist: $dist_dir"
  exit 1
fi

# An empty directory argument builds the project root, including middleware.
if [ -z "$dist_dir" ] && [ ! -f edgeone.json ]; then
  echo "::error::Project deployment requires a prepared root edgeone.json"
  exit 1
fi

deploy_command=(npx --yes edgeone@latest makers deploy)
if [ -n "$dist_dir" ]; then
  deploy_command+=("$dist_dir")
fi
deploy_command+=(-n "$project" -t "$EDGEONE_API_TOKEN" -e "$environment" -a "$area" --json)

is_transient_edgeone_failure() {
  local log_file="$1"
  grep -Eiq 'ECONNRESET|ECONNABORTED|ETIMEDOUT|EAI_AGAIN|ENOTFOUND|AxiosError: timeout|timeout of [0-9]+ms exceeded|timed out|network is unreachable|API request failed \(5[0-9][0-9]\)|API request failed \(522\)|API request failed \(524\)|HTTP 5[0-9][0-9]|502|503|504|522|524' "$log_file"
}

rm -f "$result_path" "${result_path}.failure.log"

for attempt in 1 2 3 4 5; do
  attempt_log="${result_path}.attempt-${attempt}.log"
  rm -f "$attempt_log"
  echo "EdgeOne deploy attempt ${attempt}/5 for project '${project}' in area '${area}'."

  if PAGES_SOURCE=skills "${deploy_command[@]}" 2>&1 | tee "$attempt_log"; then
    if ! node "$script_dir/parse-edgeone-deploy-result.mjs" "$attempt_log" > "$result_path"; then
      cp "$attempt_log" "${result_path}.failure.log"
      echo "::error::EdgeOne deploy command exited successfully without a valid structured success result."
      exit 1
    fi
    echo "EdgeOne deploy succeeded on attempt ${attempt}."
    exit 0
  fi

  cp "$attempt_log" "${result_path}.failure.log"
  if ! is_transient_edgeone_failure "$attempt_log"; then
    echo "::error::EdgeOne deploy failed with a non-transient error on attempt ${attempt}; not retrying."
    exit 1
  fi

  if [ "$attempt" -eq 5 ]; then
    echo "::error::EdgeOne deploy failed after 5 transient attempts."
    exit 1
  fi

  sleep_seconds=$((attempt * attempt * 10))
  echo "::warning::Transient EdgeOne deploy failure; retrying in ${sleep_seconds}s."
  sleep "$sleep_seconds"
done
