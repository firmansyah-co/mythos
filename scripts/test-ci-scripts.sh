#!/usr/bin/env bash
# Tests of scripts/ci-already-verified.sh and scripts/ci-rerun-unacquired.sh against a stand-in
# `gh` that answers from files, so every refusal can be provoked without touching GitHub.
# Needs bash and jq. Run by .github/workflows/ci-scripts.yml when either script changes.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
command -v jq >/dev/null || { echo "FAIL: jq is needed to run these tests"; exit 1; }
WORK="$(mktemp -d)"
trap 'rm -r "$WORK"' EXIT
FAILED=0

# The stand-in: `gh api [-X POST] <path> [--jq <filter>]`. A path with no file is an API failure.
mkdir -p "$WORK/bin"
cat >"$WORK/bin/gh" <<'EOF'
#!/usr/bin/env bash
[ "$1" = "api" ] || exit 2
shift
METHOD=GET; PATH_=""; FILTER="."
while [ $# -gt 0 ]; do
    case "$1" in
    -X) METHOD="$2"; shift 2 ;;
    --jq) FILTER="$2"; shift 2 ;;
    *) PATH_="$1"; shift ;;
    esac
done
if [ "$METHOD" = "POST" ]; then echo "$PATH_" >>"$FIXTURES/posted"; exit 0; fi
FILE="$FIXTURES/$(printf '%s' "$PATH_" | tr -c 'A-Za-z0-9._-' '_')"
[ -f "$FILE" ] || exit 1
jq -r "$FILTER" "$FILE"
EOF
chmod +x "$WORK/bin/gh"
put() { # api path, json
    printf '%s' "$2" >"$FIXTURES/$(printf '%s' "$1" | tr -c 'A-Za-z0-9._-' '_')"
}
expect() { # name, wanted first line, command...
    local name="$1" wanted="$2" out
    shift 2
    out="$(PATH="$WORK/bin:$PATH" FIXTURES="$FIXTURES" "$@" 2>&1)"
    if [ "$(printf '%s\n' "$out" | head -1)" = "$wanted" ]; then
        echo "ok   $name"
    else
        echo "FAIL $name: wanted '$wanted', got:"
        printf '%s\n' "$out" | sed 's/^/       /'
        FAILED=1
    fi
}

# ---- ci-already-verified.sh -------------------------------------------------------------------
R=o/r; SHA=aaa; HEAD=bbb; PARENT=ccc
verified_world() { # a merge whose tree was tested and passed; each test then breaks one thing
    FIXTURES="$WORK/v$1"; mkdir -p "$FIXTURES"
    put "repos/$R/commits/$SHA/pulls" "[{\"number\":7,\"merged_at\":\"t\",\"merge_commit_sha\":\"$SHA\",\"base\":{\"ref\":\"dev\"},\"head\":{\"sha\":\"$HEAD\",\"ref\":\"feat/x\",\"repo\":{\"full_name\":\"$R\"}}}]"
    put "repos/$R/git/commits/$SHA" "{\"tree\":{\"sha\":\"T1\"},\"parents\":[{\"sha\":\"$PARENT\"}]}"
    put "repos/$R/git/commits/$HEAD" '{"tree":{"sha":"T1"}}'
    put "repos/$R/compare/$PARENT...$HEAD" '{"behind_by":0}'
    put "repos/$R/pulls?state=all&head=o:feat/x&per_page=100" "[{\"head\":{\"sha\":\"$HEAD\"},\"base\":{\"ref\":\"dev\"}}]"
    put "repos/$R/actions/workflows/ci.yml/runs?head_sha=$HEAD&event=pull_request&status=success&per_page=100" \
        "{\"workflow_runs\":[{\"conclusion\":\"success\",\"head_branch\":\"feat/x\",\"head_repository\":{\"full_name\":\"$R\"},\"html_url\":\"u\"}]}"
}
V="$HERE/ci-already-verified.sh"
verified_world 1
expect "a tree tested and passed on its pull request is verified" "verified=true" "$V" $R $SHA ci.yml
verified_world 2; put "repos/$R/git/commits/$HEAD" '{"tree":{"sha":"T2"}}'
expect "a tree that is not the one tested is not verified" "verified=false" "$V" $R $SHA ci.yml
verified_world 3; put "repos/$R/compare/$PARENT...$HEAD" '{"behind_by":2}'
expect "a head that did not contain dev is not verified" "verified=false" "$V" $R $SHA ci.yml
verified_world 4; put "repos/$R/pulls?state=all&head=o:feat/x&per_page=100" "[{\"head\":{\"sha\":\"$HEAD\"},\"base\":{\"ref\":\"dev\"}},{\"head\":{\"sha\":\"$HEAD\"},\"base\":{\"ref\":\"feat/base\"}}]"
expect "a head also offered to another base is not verified" "verified=false" "$V" $R $SHA ci.yml
verified_world 5; put "repos/$R/actions/workflows/ci.yml/runs?head_sha=$HEAD&event=pull_request&status=success&per_page=100" '{"workflow_runs":[]}'
expect "a head with no passed run is not verified" "verified=false" "$V" $R $SHA ci.yml
verified_world 6; put "repos/$R/actions/workflows/ci.yml/runs?head_sha=$HEAD&event=pull_request&status=success&per_page=100" \
    "{\"workflow_runs\":[{\"conclusion\":\"success\",\"head_branch\":\"other\",\"head_repository\":{\"full_name\":\"$R\"},\"html_url\":\"u\"}]}"
expect "a passed run from another branch does not verify it" "verified=false" "$V" $R $SHA ci.yml
verified_world 7; put "repos/$R/commits/$SHA/pulls" "[]"
expect "a commit pushed without a pull request is not verified" "verified=false" "$V" $R $SHA ci.yml
verified_world 8; rm "$FIXTURES/$(printf '%s' "repos/$R/compare/$PARENT...$HEAD" | tr -c 'A-Za-z0-9._-' '_')"
expect "an answer that cannot be had is not verified" "verified=false" "$V" $R $SHA ci.yml

# ---- ci-rerun-unacquired.sh -------------------------------------------------------------------
NOTE='[{"message":"The job was not acquired by Runner of type hosted even after multiple attempts"}]'
rerun_world() { # a failed first attempt whose only unfinished job was never acquired
    FIXTURES="$WORK/r$1"; mkdir -p "$FIXTURES"
    put "repos/$R/actions/runs/9/attempts/$2" '{"conclusion":"failure"}'
    put "repos/$R/actions/runs/9/attempts/$2/jobs?per_page=100" \
        '{"jobs":[{"id":1,"name":"linux","conclusion":"success","steps":[{}]},{"id":2,"name":"macos","conclusion":"cancelled","steps":[]}]}'
    put "repos/$R/check-runs/2/annotations" "$NOTE"
}
posted() { [ -f "$FIXTURES/posted" ] && cat "$FIXTURES/posted" || echo nothing; }
check_posted() { # name, wanted
    if [ "$(posted)" = "$2" ]; then echo "ok   $1"; else echo "FAIL $1: sent '$(posted)', wanted '$2'"; FAILED=1; fi
}
U="$HERE/ci-rerun-unacquired.sh"
rerun_world 1 1
expect "a run whose job was never acquired is rerun" "rerun=true" "$U" $R 9 1
check_posted "and the rerun is asked of GitHub" "repos/$R/actions/runs/9/rerun-failed-jobs"
rerun_world 2 1; put "repos/$R/actions/runs/9/attempts/1/jobs?per_page=100" \
    '{"jobs":[{"id":1,"name":"linux","conclusion":"failure","steps":[{},{}]},{"id":2,"name":"macos","conclusion":"cancelled","steps":[]}]}'
expect "a run where a job really failed is not rerun" "rerun=false" "$U" $R 9 1
check_posted "and nothing is asked of GitHub" "nothing"
rerun_world 3 1; put "repos/$R/check-runs/2/annotations" '[{"message":"The operation was canceled."}]'
expect "a job somebody cancelled is not rerun" "rerun=false" "$U" $R 9 1
check_posted "and nothing is asked of GitHub" "nothing"
rerun_world 4 1; put "repos/$R/actions/runs/9/attempts/1" '{"conclusion":"cancelled"}'
expect "a cancelled run is not rerun" "rerun=false" "$U" $R 9 1
rerun_world 5 3
expect "a third attempt is not rerun again" "rerun=false" "$U" $R 9 3
check_posted "and nothing is asked of GitHub" "nothing"
rerun_world 6 1
expect "a dry run says it would rerun" "rerun=true" "$U" $R 9 1 --dry-run
check_posted "and sends nothing" "nothing"

if [ "$FAILED" = "0" ]; then echo "all passed"; else echo "FAILED"; fi
exit "$FAILED"
