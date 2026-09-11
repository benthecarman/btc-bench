# Compound approval training samples

Twelve hand-authored SFT requests, ordered from one compound approval to several interacting approvals. The user approved these examples for the small SFT pilot recorded in `compound-approvals-v1-pilot.json`.

Each request asks for Bitcoin script. The reference chooses a P2WSH witness script. Names below stand for the full public keys in the compiled fixtures; H stands for the supplied digest. These readable expressions are previews, not literal scripts to paste into a wallet.

Each example includes the request, its intended policy, compiler-produced Miniscript, and script assembly. The policy is the specification; the Miniscript is one verified implementation.

All 12 references pass the repository audit. The checks use semantic equivalence and execution with assumed-valid signatures; they do not construct signed Bitcoin transactions. No compiler fallback was required. All 12 answers extracted from the exported SFT completions also receive full marks with `grade --standard-mode`. The longest prompt plus answer is 2,714 tokens with the original SFT tokenizer; no example is truncated.

No normalized policy structure matches `evals/human-v2.json` or `evals/composition-transfer-v1.json`. This is a structural check, not a proof of semantic novelty. These examples teach the same skill family as the transfer test, so later scores on that test cannot establish unseen-family transfer.

The two five-vote examples share keys and a split group. Keep them together in any future split.

## 1. joint-seat (stage 1)

> Our club has three votes over its Bitcoin wallet: Nora has one, Eli has one, and Sam and Tess share the third. Sam and Tess must both sign to cast their shared vote. A withdrawal needs at least two votes. Sam and Tess together cannot withdraw without Nora or Eli. Can you write the Bitcoin script?

Count the joint signature pair as one vote, not two.

Policy:

```text
thresh(2,pk(Nora),pk(Eli),and(pk(Sam),pk(Tess)))
```

Verified Miniscript:

```text
thresh(2,and_b(pk(Sam),s:pk(Tess)),s:pk(Eli),s:pk(Nora))
```

Script assembly:

```text
Sam OP_CHECKSIG OP_SWAP Tess OP_CHECKSIG OP_BOOLAND OP_SWAP Eli OP_CHECKSIG OP_ADD OP_SWAP Nora OP_CHECKSIG OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## 2. delayed-seat (stage 1)

> Mina, Owen and Pia look after our reserve wallet. Every spend needs two of them. Pia's signature can count only once the output is at least 96 blocks old, measured from confirmation. Mina and Owen can spend together immediately. Please write the Bitcoin script with exactly those rules.

Apply the delay only to Pia; do not delay Mina and Owen.

Policy:

```text
thresh(2,pk(Mina),pk(Owen),and(pk(Pia),older(96)))
```

Verified Miniscript:

```text
thresh(2,pk(Mina),s:pk(Owen),snj:and_v(v:pk(Pia),older(96)))
```

Script assembly:

```text
Mina OP_CHECKSIG OP_SWAP Owen OP_CHECKSIG OP_ADD OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF Pia OP_CHECKSIGVERIFY 96 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## 3. delivery-seat (stage 1)

> For this payment, the buyer and seller can settle together. Otherwise the inspector can join either one, but the inspector's approval also needs the 32-byte delivery secret whose SHA-256 is H. Nobody can spend alone, and the secret without the inspector's signature does not count as an approval. Give me the Bitcoin script.

Keep the secret attached to the inspector approval.

Policy:

```text
thresh(2,pk(Buyer),pk(Seller),and(pk(Inspector),sha256(H)))
```

Verified Miniscript:

```text
thresh(2,j:and_v(v:pk(Inspector),sha256(H)),s:pk(Seller),s:pk(Buyer))
```

Script assembly:

```text
OP_SIZE OP_0NOTEQUAL OP_IF Inspector OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_SHA256 H OP_EQUAL OP_ENDIF OP_SWAP Seller OP_CHECKSIG OP_ADD OP_SWAP Buyer OP_CHECKSIG OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## 4. three-partnerships (stage 2)

> Three small partnerships share a Bitcoin reserve: North, South and West. North and South each have two partners; West has three. All partners in a partnership must sign for it to approve. Any two partnerships can release the funds. One signature from each of the three partnerships must not be enough. Please write the Bitcoin script.

Count complete partnerships, even when their sizes differ.

Policy:

```text
thresh(2,and(pk(North1),pk(North2)),and(pk(South1),pk(South2)),and(pk(West1),and(pk(West2),pk(West3))))
```

Verified Miniscript:

```text
thresh(2,and_b(pk(North1),s:pk(North2)),a:and_b(pk(South1),s:pk(South2)),ajc:and_v(v:pk(West1),and_v(v:pk(West2),pk_k(West3))))
```

Script assembly:

```text
North1 OP_CHECKSIG OP_SWAP North2 OP_CHECKSIG OP_BOOLAND OP_TOALTSTACK South1 OP_CHECKSIG OP_SWAP South2 OP_CHECKSIG OP_BOOLAND OP_FROMALTSTACK OP_ADD OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF West1 OP_CHECKSIGVERIFY West2 OP_CHECKSIGVERIFY West3 OP_CHECKSIG OP_ENDIF OP_FROMALTSTACK OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## 5. chair-delegate (stage 2)

> Our nonprofit needs two of three approvals to spend. The board approval needs the chair plus either the deputy or secretary; the chair signing with both the deputy and secretary still gives only one board approval. The treasurer can give the second kind of approval alone. The auditor can give the third with their signature and the 32-byte secret matching SHA-256 H. Can you write the Bitcoin script?

Keep an either/or choice inside one approval, rather than counting it twice.

Policy:

```text
thresh(2,and(pk(Chair),or(pk(Deputy),pk(Secretary))),pk(Treasurer),and(pk(Auditor),sha256(H)))
```

Verified Miniscript:

```text
thresh(2,and_b(or_b(pk(Deputy),s:pk(Secretary)),s:pk(Chair)),s:pk(Treasurer),aj:and_v(v:pk(Auditor),sha256(H)))
```

Script assembly:

```text
Deputy OP_CHECKSIG OP_SWAP Secretary OP_CHECKSIG OP_BOOLOR OP_SWAP Chair OP_CHECKSIG OP_BOOLAND OP_SWAP Treasurer OP_CHECKSIG OP_ADD OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF Auditor OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_SHA256 H OP_EQUAL OP_ENDIF OP_FROMALTSTACK OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## 6. two-teams-delayed-person (stage 2)

> We want two approvals for a payment: our legal team, our finance team, or our trustee. Each team has two keys and must provide both signatures to approve. The trustee uses one signature, but it can count only when the output is at least 288 blocks old. The two teams can approve together immediately. Give me the Bitcoin script.

A team is one approval; its members cannot replace the trustee or another team.

Policy:

```text
thresh(2,and(pk(Legal1),pk(Legal2)),and(pk(Finance1),pk(Finance2)),and(pk(Trustee),older(288)))
```

Verified Miniscript:

```text
thresh(2,and_b(pk(Legal1),s:pk(Legal2)),a:and_b(pk(Finance1),s:pk(Finance2)),snj:and_v(v:pk(Trustee),older(288)))
```

Script assembly:

```text
Legal1 OP_CHECKSIG OP_SWAP Legal2 OP_CHECKSIG OP_BOOLAND OP_TOALTSTACK Finance1 OP_CHECKSIG OP_SWAP Finance2 OP_CHECKSIG OP_BOOLAND OP_FROMALTSTACK OP_ADD OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF Trustee OP_CHECKSIGVERIFY 288 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## 7. separate-delays (stage 3)

> Ada, Ben and Cleo each have a vote on this Bitcoin reserve. We need any two votes. Ada's signature can count once the output is at least 48 blocks old; Ben's needs at least 144 blocks. Cleo can approve without a delay, but her signature must come with the 32-byte secret matching SHA-256 H. Please write the script. All ages are measured from confirmation.

Preserve separate conditions on each vote, including the earlier Ada-plus-Cleo path.

Policy:

```text
thresh(2,and(pk(Ada),older(48)),and(pk(Ben),older(144)),and(pk(Cleo),sha256(H)))
```

Verified Miniscript:

```text
thresh(2,j:and_v(v:pk(Cleo),sha256(H)),snj:and_v(v:pk(Ben),older(144)),snj:and_v(v:pk(Ada),older(48)))
```

Script assembly:

```text
OP_SIZE OP_0NOTEQUAL OP_IF Cleo OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_SHA256 H OP_EQUAL OP_ENDIF OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF Ben OP_CHECKSIGVERIFY 144 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_ADD OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF Ada OP_CHECKSIGVERIFY 48 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## 8. committee-custodian-trustee (stage 3)

> Two of these three parties must approve a withdrawal: the committee, the custodian, or the trustee. The committee needs two of its three members to sign and counts as one party. The custodian needs a signature plus the 32-byte secret matching HASH160 H. The trustee needs a signature and a spending transaction with block-height locktime at least 991250. Write the Bitcoin script; there should be no other way to spend.

A committee quorum remains one outer vote; preserve the hash and absolute lock.

Policy:

```text
thresh(2,thresh(2,pk(Committee1),pk(Committee2),pk(Committee3)),and(pk(Custodian),hash160(H)),and(pk(Trustee),after(991250)))
```

Verified Miniscript:

```text
thresh(2,multi(2,Committee1,Committee2,Committee3),aj:and_v(v:pk(Custodian),hash160(H)),snj:and_v(v:pk(Trustee),after(991250)))
```

Script assembly:

```text
OP_PUSHNUM_2 Committee1 Committee2 Committee3 OP_PUSHNUM_3 OP_CHECKMULTISIG OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF Custodian OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_HASH160 H OP_EQUAL OP_ENDIF OP_FROMALTSTACK OP_ADD OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF Trustee OP_CHECKSIGVERIFY 991250 OP_CLTV OP_ENDIF OP_0NOTEQUAL OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## 9. relative-time-votes (stage 3)

> Alex, Blair and Casey manage this reserve, and any two must sign to spend. Alex's signature can count only after a relative time lock of at least 1024 seconds, and Blair's after at least 3072 seconds. These are two and six 512-second units on Bitcoin's relative-lock clock. Casey's signature has no time condition. Please write the Bitcoin script.

Use relative-time units, not blocks, and retain each signer's own minimum.

Policy:

```text
thresh(2,and(pk(Alex),older(4194306)),and(pk(Blair),older(4194310)),pk(Casey))
```

Verified Miniscript:

```text
thresh(2,nj:and_v(v:pk(Alex),older(4194306)),snj:and_v(v:pk(Blair),older(4194310)),s:pk(Casey))
```

Script assembly:

```text
OP_SIZE OP_0NOTEQUAL OP_IF Alex OP_CHECKSIGVERIFY 4194306 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF Blair OP_CHECKSIGVERIFY 4194310 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_ADD OP_SWAP Casey OP_CHECKSIG OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## 10. five-votes-2 (stage 4)

> Our reserve has five votes: the founder, the treasurer, the North team, the South team, and the trustee. Each team has two members who must both sign to cast its one vote. The founder and treasurer each use one signature. The trustee's signature counts only once the output is at least 432 blocks old. A withdrawal needs at least 2 of the five votes. Please write the Bitcoin script. Extra votes are fine, but extra members of one team do not create extra votes.

Change the outer vote count while keeping the groups and their conditions intact.

Policy:

```text
thresh(2,pk(Founder),pk(Treasurer),and(pk(North1),pk(North2)),and(pk(South1),pk(South2)),and(pk(Trustee),older(432)))
```

Verified Miniscript:

```text
thresh(2,and_b(pk(North1),s:pk(North2)),s:pk(Treasurer),s:pk(Founder),a:and_b(pk(South1),s:pk(South2)),snj:and_v(v:pk(Trustee),older(432)))
```

Script assembly:

```text
North1 OP_CHECKSIG OP_SWAP North2 OP_CHECKSIG OP_BOOLAND OP_SWAP Treasurer OP_CHECKSIG OP_ADD OP_SWAP Founder OP_CHECKSIG OP_ADD OP_TOALTSTACK South1 OP_CHECKSIG OP_SWAP South2 OP_CHECKSIG OP_BOOLAND OP_FROMALTSTACK OP_ADD OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF Trustee OP_CHECKSIGVERIFY 432 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## 11. five-votes-3 (stage 4)

> Our reserve has five votes: the founder, the treasurer, the North team, the South team, and the trustee. Each team has two members who must both sign to cast its one vote. The founder and treasurer each use one signature. The trustee's signature counts only once the output is at least 432 blocks old. A withdrawal needs at least 3 of the five votes. Please write the Bitcoin script. Extra votes are fine, but extra members of one team do not create extra votes.

Change the outer vote count while keeping the groups and their conditions intact.

Policy:

```text
thresh(3,pk(Founder),pk(Treasurer),and(pk(North1),pk(North2)),and(pk(South1),pk(South2)),and(pk(Trustee),older(432)))
```

Verified Miniscript:

```text
thresh(3,and_b(pk(North1),s:pk(North2)),s:pk(Treasurer),s:pk(Founder),a:and_b(pk(South1),s:pk(South2)),snj:and_v(v:pk(Trustee),older(432)))
```

Script assembly:

```text
North1 OP_CHECKSIG OP_SWAP North2 OP_CHECKSIG OP_BOOLAND OP_SWAP Treasurer OP_CHECKSIG OP_ADD OP_SWAP Founder OP_CHECKSIG OP_ADD OP_TOALTSTACK South1 OP_CHECKSIG OP_SWAP South2 OP_CHECKSIG OP_BOOLAND OP_FROMALTSTACK OP_ADD OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF Trustee OP_CHECKSIGVERIFY 432 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_ADD OP_PUSHNUM_3 OP_EQUAL
```

## 12. shared-secret (stage 4)

> Uma and Vik each have an approval key, and either one's approval needs their own signature plus the same 32-byte secret matching SHA-256 H. Wren has a third approval: her signature can count once the output is at least 240 blocks old. Any two approvals can spend. Uma and Vik together can spend immediately if they know the secret. The secret by itself gives no votes. Please write the Bitcoin script.

Sharing a secret does not merge votes or provide missing signatures.

Policy:

```text
thresh(2,and(pk(Uma),sha256(H)),and(pk(Vik),sha256(H)),and(pk(Wren),older(240)))
```

Verified Miniscript:

```text
thresh(2,j:and_v(v:pk(Uma),sha256(H)),aj:and_v(v:pk(Vik),sha256(H)),snj:and_v(v:pk(Wren),older(240)))
```

Script assembly:

```text
OP_SIZE OP_0NOTEQUAL OP_IF Uma OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_SHA256 H OP_EQUAL OP_ENDIF OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF Vik OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_SHA256 H OP_EQUAL OP_ENDIF OP_FROMALTSTACK OP_ADD OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF Wren OP_CHECKSIGVERIFY 240 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## Rebuild and verify

Run these commands from the repository root. Output paths must not already exist.

```sh
cargo run --release -p bench-cli --example compile_training_catalog -- \
  training/compound-approvals-v1-samples.json \
  datasets/compound-approvals-v1-samples
target/release/btc-bench audit --dataset datasets/compound-approvals-v1-samples
/home/ben/.local/share/venvs/sft/bin/python scripts/rl_prepare.py \
  --pool datasets/compound-approvals-v1-samples \
  --out datasets/rl-compound-approvals-v1-samples.jsonl \
  --model runs/sft-qwen3-4b-think/merged --thinking
/home/ben/.local/share/venvs/sft/bin/python scripts/prepare_composed_sft.py \
  --data datasets/rl-compound-approvals-v1-samples.jsonl \
  --pool datasets/compound-approvals-v1-samples \
  --out datasets/sft-compound-approvals-v1-samples.jsonl
```

The local training files are `datasets/rl-compound-approvals-v1-samples.jsonl` (rendered prompts) and `datasets/sft-compound-approvals-v1-samples.jsonl` (policy → Miniscript → assembly targets in the existing thinking/submit format). They use the tokenizer from `runs/sft-qwen3-4b-think/merged`.
