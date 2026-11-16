# The self-hosted runner image

The Linux jobs of CI run on `arc-runner-org`, a runner scale set (Actions Runner Controller) in
the k3s cluster on the development machine. This directory is what its runners start from.

| File | What it is |
|---|---|
| `packages.txt` | The system packages the Rust job needs. Read by the Dockerfile and by the job's install step, so there is one list. |
| `Dockerfile` | GitHub's runner image plus those packages. |
| `build.sh` | Builds the image with podman, checks it, and pushes it to the local registry. |
| `registry.yaml` | The registry the cluster pulls it from, on `127.0.0.1:5000` of the host only. |
| `mythos-ci-runner-image.service`, `.timer` | systemd user units that run `build.sh` weekly. |

## Why the image is rebuilt every week

GitHub stops giving jobs to a self-hosted runner that is more than 30 days behind the current
runner release. The stock image was pulled as `latest` on every start, so it kept up by itself.
This one keeps up because `build.sh` pulls the base again on every build and the timer runs it
weekly. **If the timer stops, CI keeps working for a while and then stops being given jobs.**

Check it:

```bash
systemctl --user list-timers mythos-ci-runner-image.timer
journalctl --user -u mythos-ci-runner-image.service -n 20
curl -s http://127.0.0.1:5000/v2/mythos-arc-runner/tags/list     # latest, and runner-<version>
```

## Changing the packages

Edit `packages.txt` and merge. Until the next build, the job installs the missing package with
apt on each run (slower, never wrong). To bake it at once: `ci/runner/build.sh`.

## Setting it up on a machine (once)

```bash
kubectl apply -f ci/runner/registry.yaml
ci/runner/build.sh

# Point the scale set at the image, and bound how many runners share the machine.
kubectl patch autoscalingrunnerset arc-runner-org -n arc-runners --type=json -p '[
  {"op":"replace","path":"/spec/maxRunners","value":2},
  {"op":"replace","path":"/spec/template/spec/containers/0/image","value":"localhost:5000/mythos-arc-runner:latest"},
  {"op":"add","path":"/spec/template/spec/containers/0/imagePullPolicy","value":"Always"}
]'

cp ci/runner/mythos-ci-runner-image.service ci/runner/mythos-ci-runner-image.timer ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now mythos-ci-runner-image.timer
```

The scale set is patched with `kubectl`, not `helm upgrade`: its live spec already differs from
the Helm release's values (the tool cache mount under `/opt/hostedtoolcache` exists only in the
live spec), so `helm upgrade --reuse-values` would remove that mount and with it rustup, sccache
and the pnpm store. The registry's host path and the timer's checkout path name this machine's
home directory; change them on another machine.

## Why two runners at most

One CI run has two jobs for this runner (Rust, frontend). With ten allowed, two or three runs at
once put six or more builds on one machine, and the SHM latency gate, which measures that
machine, missed its bound. At two, a second run waits its turn instead.

## Going back

```bash
kubectl patch autoscalingrunnerset arc-runner-org -n arc-runners --type=json -p '[
  {"op":"replace","path":"/spec/template/spec/containers/0/image","value":"ghcr.io/actions/actions-runner:latest"}
]'
```

The install step then installs the packages in every run again, as it did before.
