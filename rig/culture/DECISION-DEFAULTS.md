# Deciding without me

> **Provenance.** The substance of this file is Adrian's instruction of 2026-11-11. The drafting
> is his assistant's. First-person voice here transcribes what he said; it is not his authorship.
> Where a clause below goes beyond his words, it says so.

Adrian, 2026-11-11.

I want the questions to keep coming, and I do not want the team to stall when I am
not looking. So: ask me, wait, and then decide. This file says how long to wait and
what you may decide alone.

## 1. Ask first, always

Nothing here changes how a question reaches me. `advisor` presents it with what is
being decided, the options and their trade-offs, the recommendation, and what changes
once I decide. A question without a recommendation has outsourced the judgement back
to me — do not do that.

## 2. How long to wait

**24 hours by default. 2 hours when there is nothing else to do.**

Stack the questions. Several open decisions is normal and expected, not a failure.
Keep one running list, newest decision-bearing first, and present it when I ask or
when it changes materially. Do not block a later question on an earlier unanswered
one, and do not make me reconstruct the list from the queue.

### The two clocks

**Busy clock — 24 hours.** While the team has other actionable work, an unanswered
question waits a day. Work continues on everything else in the meantime.

**Idle clock — 2 hours.** If a question is the only thing standing between the team
and having nothing to do, waiting a day is waste. Once the team is idle and the
question is the reason, the clock shortens to 2 hours.

### How to tell which clock applies

```bash
rig queue list --json | jq -r '.[] | select(.state == "pending" or .state == "in-progress")'
```

- **Any other actionable row exists** → 24 hours.
- **No other actionable row exists** → 2 hours from when the team went idle.

Do not count these as "work" for the busy clock:

- a row blocked on something outside the team's control;
- a row parked on a decision that is itself waiting on me;
- a row waiting on another row that is waiting on me.

Otherwise two questions waiting on each other look like a busy team and neither ever
becomes answerable. When the only remaining rows are all blocked on me, that is the
idle case: 2 hours.

State which clock applies, and the deadline, when you ask.

## 3. What you may decide alone

**Everything except the `release` branch.**

| Act | Decide alone? |
|---|---|
| `dev` changes, slices, refactors, architecture | **yes** |
| promoting `dev` → `rc` | **yes** |
| anything touching the `release` branch | **no — waits for me, however long** |
| a release tag (`vX.Y.Z`) or a published artifact | **no** |
| a force-push, or deleting work another seat produced | **no** |

`release_auditor` keeps its own gate on `rc` → `release` regardless of this file.
`release_auditor` may proceed on `rc` without waiting for me; `release` is not reachable by
default, by anyone.

**This file says whether an act may proceed without an answer; it does not change who may
perform it (§6) or whether it must be reviewed (§6).**

If your recommendation needs a release-branch act, do the reversible part now and park
at that step with a named blocker. Do not let the whole slice wait because its last
step does.

## 4. What makes a recommendation executable

You may act on a timeout only if the brief you already gave me contains all of:

- the recommendation, named as one option;
- the evidence **you verified yourself** — a file and line, a command you ran, a
  measurement — not a quote from another seat's summary;
- what being wrong would cost, and how it would be noticed;
- whether it is reversible, and by what.

If any of those is missing, that is what you report. "I cannot decide this by default
because I have not verified X" is a complete answer, and a better one than a confident
guess.

## 5. When you decide on a timeout

Record it on the row, in this shape:

```
DECIDED BY DEFAULT — no answer from Adrian within <clock>.
Clock: <24h default | 2h idle — team had no other actionable work>
Recommendation executed: <option letter and one line>
Evidence I verified before acting: <file:line, command, measurement>
What would reverse it: <condition>
Reversible: <yes, by X | no, because Y>
```

Then do the work. Do not ask again in the same breath; a second ask resets nothing and
only adds noise.

## 6. Tell me afterwards

Every defaulted decision appears once, in your next status summary:

    DEFAULTED: <decision> — <recommendation> — <when> — <reversible?>

so I can review what you called without me and reverse what I disagree with. If I
reverse one, that reversal is the answer to the question, and it also tells you the
recommendation was wrong in a way worth recording.
