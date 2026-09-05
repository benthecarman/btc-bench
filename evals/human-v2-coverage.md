# Human-v2 coverage audit

160 requests: 120 write and 40 tree. The original 32 catalog entries are unchanged. The 128 additions form 64 pairs; the full set has 88 scenario groups.

These are fixed synthetic requests. A pair changes spending conditions, not just keys or wording. Groups can still share structures with other groups. The count of groups is not a count of independent observations.

| Subset | Questions | Operator shapes | Questions with a shape in checked SFT traces |
|---|---:|---:|---:|
| Original | 32 | 26 | 14 |
| Added | 128 | 122 | 14 |
| All write | 120 | 104 | 17 |
| All tree | 40 | 40 | 11 |
| Full set | 160 | 141 | 28 |

The additions include **115 operator shapes absent from human-v1**. 121 of the added requests have such a shape.

## Method and limits

Shapes erase key identity, hash values and lock values. They retain hash functions, absolute/relative lock domains, thresholds and repeated operands. AND/OR order is sorted and nested uses are flattened. Thresholds of one or all become OR or AND. This is a conservative operator comparison: it loses repeated-key roles and does not prove semantic novelty. It also does not equate all logically equivalent forms.

The training check found policy lines in 14700/16696 rows of `datasets/sft-train-think.jsonl` (488 shapes). The four policy-line markers from `sft_traces.py` are recognized; other rows are excluded. This checks that stored file, not all historical training inputs. It does not certify a blind holdout.

The new cases have no measured model score yet. Report correctness and tree weight separately. Keep pairs together when splitting data or estimating uncertainty; group further by policy family for a family-transfer experiment.

## Provenance

- `evals/human-v2.json` SHA-256: `1ebf73d5b9f6ae21b33a9ab3b7d72ecfb764c0fc06e0958a1d2586d789868772`
- `evals/human-v1.json` SHA-256: `72e1f03e64fbf01484f7ecb8e3763aeefa6c0da1c226d45a106cd05925618d32`
- `datasets/sft-train-think.jsonl` SHA-256: `22dd01f38fbc37705efc512542e61e5b30de4381f37709b422811d3bc7768e7f`

Reproduce from the repository root:

```bash
python scripts/audit_human_catalog.py
```
