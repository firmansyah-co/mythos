#!/usr/bin/env bash
# Run a CI run's jobs again when the only thing wrong with it is that GitHub never started them.
#
#   scripts/ci-rerun-unacquired.sh <owner/repo> <run id> <attempt> [--dry-run]
#
# WHY: a job for a hosted runner that GitHub cannot place ends, after fifteen minutes in the queue,
# as `cancelled` with no steps and the annotation "The job was not acquired by Runner of type
# hosted even after multiple attempts". The run is then a failure, though no code was ever run.
# On 2026-11-16 three CI runs failed for this alone, each time on the macOS job. It is a required
# check, so a pull request sat blocked until someone noticed and pressed "re-run failed jobs",
# and `dev` stayed red for over three hours once.
#
# It reruns ONLY when every job that did not succeed was never acquired. A job that ran and
# failed, or a run somebody cancelled, is left alone: that needs a person, not a retry.
# At most MAX_ATTEMPTS attempts in all, so an outage cannot loop.
set -u

REPO="${1:?owner/repo}"
RUN="${2:?run id}"
ATTEMPT="${3:?attempt}"
DRY="${4:-}"
MAX_ATTEMPTS=3
NOT_ACQUIRED="was not acquired by Runner"

say() { # decision, reason
    echo "rerun=$1"
    echo "$2"
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then echo "$2" >>"$GITHUB_STEP_SUMMARY"; fi
}
ask() { gh api "$@" 2>/dev/null; }

CONCLUSION=$(ask "repos/$REPO/actions/runs/$RUN/attempts/$ATTEMPT" --jq '.conclusion') || {
    say false "Could not read run $RUN attempt $ATTEMPT."
    exit 0
}
if [ "$CONCLUSION" != "failure" ]; then
    say false "Run $RUN attempt $ATTEMPT concluded '$CONCLUSION', not 'failure': left alone."
    exit 0
fi
if [ "$ATTEMPT" -ge "$MAX_ATTEMPTS" ]; then
    say false "Run $RUN is already at attempt $ATTEMPT of $MAX_ATTEMPTS: left for a person."
    exit 0
fi

JOBS=$(ask "repos/$REPO/actions/runs/$RUN/attempts/$ATTEMPT/jobs?per_page=100" --jq \
    '.jobs[] | select(.conclusion != "success" and .conclusion != "skipped") | "\(.id)\t\(.conclusion)\t\(.steps | length)\t\(.name)"') || {
    say false "Could not list the jobs of run $RUN attempt $ATTEMPT."
    exit 0
}
if [ -z "$JOBS" ]; then
    say false "Run $RUN attempt $ATTEMPT failed but lists no job that did: left alone."
    exit 0
fi

UNACQUIRED=""
while IFS=$'\t' read -r ID JOB_CONCLUSION STEPS NAME; do
    NOTES=""
    if [ "$JOB_CONCLUSION" = "cancelled" ] && [ "$STEPS" = "0" ]; then
        NOTES=$(ask "repos/$REPO/check-runs/$ID/annotations" --jq '.[].message') || NOTES=""
    fi
    case "$NOTES" in
    *"$NOT_ACQUIRED"*) UNACQUIRED="$UNACQUIRED '$NAME'" ;;
    *)
        say false "Job '$NAME' of run $RUN ended '$JOB_CONCLUSION' after $STEPS steps: a real result, not rerun."
        exit 0
        ;;
    esac
done <<<"$JOBS"

if [ "$DRY" = "--dry-run" ]; then
    say true "Would rerun, never acquired by a runner:$UNACQUIRED (dry run, nothing sent)."
    exit 0
fi
if gh api -X POST "repos/$REPO/actions/runs/$RUN/rerun-failed-jobs" >/dev/null; then
    say true "Rerun requested, never acquired by a runner:$UNACQUIRED."
else
    say false "The rerun request for run $RUN was refused."
    exit 1
fi
