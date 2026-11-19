#!/usr/bin/env bash
# Was the tree of this commit on `dev` already verified by CI on the pull request that produced it?
#
#   scripts/ci-already-verified.sh <owner/repo> <commit sha> <ci workflow file>
#
# WHY: `dev` requires its checks to pass on a branch that is up to date with it, so a pull request
# is tested on exactly the content its merge puts on `dev`. The push that follows the merge then
# ran the whole suite again on the same tree: measured over 100 runs on 2026-11-16, 15 of the 37
# CI runs were that second run, each one four jobs and about seven minutes on the shared runner.
#
# It answers `verified=true` only when ALL of this is established from GitHub, never assumed:
#   1. the commit is the merge commit of a merged pull request into dev;
#   2. its tree is the tree of that pull request's head (what was tested is what landed);
#   3. the commit's parent on dev is contained in that head, so every CI run of that head tested
#      the head's own tree (a pull request run tests head merged into base);
#   4. no other pull request offered that same head to another base (a run for one would have
#      tested the head merged into that base, which is another tree);
#   5. a `pull_request` run of the CI workflow from that branch, at that head, succeeded.
# Anything else, including any answer it cannot get, is `verified=false`: the suite runs.
# A direct push, a merge of a branch that was behind, or a rerun that went red all land there.
set -u

REPO="${1:?owner/repo}"
SHA="${2:?commit sha}"
WORKFLOW="${3:?ci workflow file}"

finish() { # verified, reason
    echo "verified=$1"
    echo "$2"
    if [ -n "${GITHUB_OUTPUT:-}" ]; then echo "verified=$1" >>"$GITHUB_OUTPUT"; fi
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        if [ "$1" = "true" ]; then
            echo "**The suite was not run again.** $2" >>"$GITHUB_STEP_SUMMARY"
        else
            echo "**The suite runs.** $2" >>"$GITHUB_STEP_SUMMARY"
        fi
    fi
    exit 0
}
ask() { gh api "$@" 2>/dev/null; }

PR_LINE=$(ask "repos/$REPO/commits/$SHA/pulls" --jq \
    "[.[] | select(.merged_at != null and .merge_commit_sha == \"$SHA\" and .base.ref == \"dev\")][0] | select(. != null) | \"\(.number) \(.head.sha) \(.head.repo.full_name) \(.head.ref)\"") ||
    finish false "Could not ask which pull request produced $SHA."
[ -n "$PR_LINE" ] || finish false "$SHA is not the merge commit of a merged pull request into dev."
read -r PR HEAD HEAD_REPO HEAD_REF <<<"$PR_LINE"
[ -n "$HEAD_REF" ] || finish false "Could not read the head of pull request #$PR."

COMMIT=$(ask "repos/$REPO/git/commits/$SHA" --jq '"\(.tree.sha) \(.parents[0].sha)"') ||
    finish false "Could not read commit $SHA."
TREE="${COMMIT%% *}"
PARENT="${COMMIT##* }"
HEAD_TREE=$(ask "repos/$REPO/git/commits/$HEAD" --jq '.tree.sha') ||
    finish false "Could not read the head $HEAD of pull request #$PR."
[ -n "$TREE" ] && [ "$TREE" = "$HEAD_TREE" ] ||
    finish false "The tree of $SHA ($TREE) is not the tree pull request #$PR was tested on ($HEAD_TREE)."

BEHIND=$(ask "repos/$REPO/compare/$PARENT...$HEAD" --jq '.behind_by') ||
    finish false "Could not compare the head of pull request #$PR with dev."
[ "$BEHIND" = "0" ] ||
    finish false "The head of pull request #$PR did not contain dev ($PARENT), so its runs tested another tree."

OTHER_BASES=$(ask "repos/$REPO/pulls?state=all&head=${HEAD_REPO%%/*}:$HEAD_REF&per_page=100" --jq \
    "[.[] | select(.head.sha == \"$HEAD\" and .base.ref != \"dev\")] | length") ||
    finish false "Could not list the pull requests of branch $HEAD_REF."
[ "$OTHER_BASES" = "0" ] ||
    finish false "Head $HEAD was also offered to a base other than dev, so a run of it may have tested another tree."

# A run names its pull request only while that one is open, so the run is matched by what stays:
# the workflow, the event, the head commit, and the branch and repository it came from.
RUN=$(ask "repos/$REPO/actions/workflows/$WORKFLOW/runs?head_sha=$HEAD&event=pull_request&status=success&per_page=100" --jq \
    "[.workflow_runs[] | select(.conclusion == \"success\" and .head_branch == \"$HEAD_REF\" and .head_repository.full_name == \"$HEAD_REPO\")][0] | select(. != null) | .html_url") ||
    finish false "Could not ask for the CI runs of $HEAD."
[ -n "$RUN" ] || finish false "No successful CI run of pull request #$PR at its head $HEAD was found."

finish true "Tree $TREE is what pull request #$PR was tested on, and that run passed: $RUN"
