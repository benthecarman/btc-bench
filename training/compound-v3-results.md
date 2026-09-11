# Gated-team and chat training results

The old development questions select the checkpoint. Fresh compositions and human-v2 do not select it.

## Development and training fit

| Model | Development chat | Development submit | Fit chat | Fit submit |
|---|---:|---:|---:|---:|
| original | 0/48 | 0/48 | 0/24 | 0/24 |
| step144 | 39/48 | 39/48 | 21/24 | 22/24 |
| step288 | 37/48 | 39/48 | 23/24 | 23/24 |

The frozen rule selects **step144**. Each development question appears in two interfaces; the 96 answers are not 96 independent questions.

## Fresh composition check

| Model and interface | New single-team arrangements | Two conditioned teams | Total |
|---|---:|---:|---:|
| original chat | 0/24 | 0/24 | 0/48 |
| original submit | 0/24 | 0/24 | 0/48 |
| selected chat | 15/24 | 20/24 | 35/48 |
| selected submit | 18/24 | 18/24 | 36/48 |

These synthetic questions have normalized structures absent from the checked training sources. Two conditioned-team approvals never occur together in this training curriculum. The templates and vocabulary remain related; this is not an independently authored human test.

## Existing human-style benchmark

| Model | Write chat | Tree chat | Write submit | Tree submit |
|---|---:|---:|---:|---:|
| original | 32/120 | 1/40 | 34/120 | 4/40 |
| selected | 34/120 | 3/40 | 33/120 | 5/40 |

All expected questions remain in the denominator, including unextractable answers. Counts measure semantic correctness; tree weight is separate. The original model server is restored. This run changes the curriculum, interface mix, replay sample, and learning rate, so it cannot isolate one cause of any gain.

See [the design](compound-v3.md), [error analysis and trace checks](compound-v3-analysis.md), and [three spot checks](compound-v3-spot-check.md). Full per-question results are in `runs/compound-v3-results.json`.
