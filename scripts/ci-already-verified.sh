#!/usr/bin/env bash
# Was the tree of this commit already verified by CI on the pull request that produced it?
#
#   scripts/ci-already-verified.sh <owner/repo> <commit sha> <ci workflow file> [<merge queue ref>]
#
# It is asked in two places, and answers the same question in both:
#
#   - for a commit the MERGE QUEUE of `dev` is about to test (the fourth argument is the ref of
#     the merge group, `refs/heads/gh-readonly-queue/dev/pr-<number>-<base>`). The queue puts each
#     pull request on top of what is ahead of it and runs the checks on that commit. When the
#     pull request already contained everything ahead of it, that commit has the tree its own
#     run tested, and testing it again says nothing new: the suite is skipped, a skipped required
#     check counts as passed, and the queue merges at once. When `dev` moved, or another pull
#     request is ahead in the queue, the tree is another one and the suite runs;
#   - for a commit pushed to `dev` (no fourth argument), so that the push after a merge does not
#     run the suite on a tree that was tested before the merge. Measured over 100 runs on
#     2026-11-16, 15 of the 37 CI runs were that second run, each one four jobs and about seven
#     minutes on the shared runner.
#
# It answers `verified=true` only when ALL of this is established from GitHub, never assumed:
#   1. the commit is the merge commit of a merged pull request into dev, or, in the queue, the
#      commit the queue made for an open pull request into dev;
#   2. its tree is the tree of that pull request's head (what was tested is what lands);
#   3. the commit's parent is contained in that head, so every CI run of that head tested the
#      head's own tree (a pull request run tests head merged into base, and the base it was
#      merged into is an ancestor of that parent);
#   4. no other pull request offered that same head to another base (a run for one would have
#      tested the head merged into that base, which is another tree);
#   5. a `pull_request` run of the CI workflow from that branch, at that head, succeeded.
# A commit pushed to dev is also verified when it is the very commit a `merge_group` run of the CI
# workflow tested and that run succeeded: either its suite ran on this commit, or this script
# established 1 to 5 for it there.
# Anything else, including any answer it cannot get, is `verified=false`: the suite runs.
# A direct push, a merge of a branch that was behind, or a rerun that went red all land there.
set -u

REPO="${1:?owner/repo}"
SHA="${2:?commit sha}"
WORKFLOW="${3:?ci workflow file}"
QUEUE_REF="${4:-}"
QUEUE_PREFIX="refs/heads/gh-readonly-queue/dev/pr-"

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

if [ -n "$QUEUE_REF" ]; then
    # In the queue: the ref names the pull request this commit was made for.
    case "$QUEUE_REF" in
    "$QUEUE_PREFIX"*) ;;
    *) finish false "$QUEUE_REF is not a merge queue ref of dev." ;;
    esac
    PR="${QUEUE_REF#"$QUEUE_PREFIX"}"
    PR="${PR%%-*}"
    case "$PR" in
    "" | *[!0-9]*) finish false "Could not read a pull request number from $QUEUE_REF." ;;
    esac
    PR_LINE=$(ask "repos/$REPO/pulls/$PR" --jq \
        "select(.state == \"open\" and .base.ref == \"dev\") | \"\(.number) \(.head.sha) \(.head.repo.full_name) \(.head.ref)\"") ||
        finish false "Could not ask for pull request #$PR."
    [ -n "$PR_LINE" ] || finish false "Pull request #$PR is not an open pull request into dev."
else
    # On dev: the commit the merge queue tested needs nothing more than that run. A run that is
    # still finishing its jobs that hold no branch is waited for, a little.
    TRIES="${CI_VERIFIED_QUEUE_TRIES:-6}"
    while [ "$TRIES" -gt 0 ]; do
        QUEUE_RUN=$(ask "repos/$REPO/actions/workflows/$WORKFLOW/runs?head_sha=$SHA&event=merge_group&per_page=100" --jq \
            "[.workflow_runs[] | select(.head_sha == \"$SHA\" and .head_repository.full_name == \"$REPO\" and (.head_branch | startswith(\"gh-readonly-queue/dev/\")))] | sort_by(.run_number) | last | select(. != null) | \"\(.status) \(.conclusion) \(.html_url)\"") || QUEUE_RUN=""
        case "$QUEUE_RUN" in
        "completed success "*)
            finish true "$SHA is the commit the merge queue tested, and that run passed: ${QUEUE_RUN##* }"
            ;;
        "" | "completed "*) break ;;
        esac
        TRIES=$((TRIES - 1))
        [ "$TRIES" -gt 0 ] && sleep "${CI_VERIFIED_QUEUE_WAIT:-20}"
    done

    PR_LINE=$(ask "repos/$REPO/commits/$SHA/pulls" --jq \
        "[.[] | select(.merged_at != null and .merge_commit_sha == \"$SHA\" and .base.ref == \"dev\")][0] | select(. != null) | \"\(.number) \(.head.sha) \(.head.repo.full_name) \(.head.ref)\"") ||
        finish false "Could not ask which pull request produced $SHA."
    [ -n "$PR_LINE" ] || finish false "$SHA is not the merge commit of a merged pull request into dev."
fi
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
    finish false "Could not compare the head of pull request #$PR with what it was put on ($PARENT)."
[ "$BEHIND" = "0" ] ||
    finish false "The head of pull request #$PR did not contain what it was put on ($PARENT), so its runs tested another tree."

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
