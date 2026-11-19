# The image the self-hosted runners (scale set arc-runner-org) start from: GitHub's runner image
# with the system packages the Rust job needs already installed.
#
# WHY: the stock image has none of them, so every run of that job installed 348 packages with apt
# before it compiled anything: a median 86 s of a 5.7 min job over 36 runs (2026-11-16), 715 s at
# worst. The job's install step checks `dpkg -s` for the same list (packages.txt) and skips when
# they are present, so with this image it has nothing to do, and with any other it still works.
#
# `latest`, deliberately: GitHub stops giving jobs to a runner that is more than 30 days behind
# the current release. build.sh pulls the base again on every build and a timer runs it weekly
# (see README.md), so this image follows the runner releases as the stock one did.
FROM ghcr.io/actions/actions-runner:latest
USER root
COPY packages.txt /tmp/packages.txt
RUN apt-get update \
 && apt-get install -y --no-install-recommends $(cat /tmp/packages.txt) \
 && rm -rf /var/lib/apt/lists/* /tmp/packages.txt
USER runner
