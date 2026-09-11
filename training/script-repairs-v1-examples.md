Six raw Bitcoin Script scenarios, each as a normal request and a repair. All have chat and submit_script targets. The policy and Miniscript form the checked explanation; the final answer is raw script ASM, with matching hex saved in the review JSON.

Prompts say Bitcoin script and let the model choose its context. These targets choose P2WSH. All paired variants belong to the same training group. The earlier wallet descriptor repair examples are a separate, shelved experiment and are not included.

**Reading the previews:** A, B and C below stand for the supplied compressed public keys, in prompt order. They are display abbreviations only; every actual training prompt, draft and answer contains full public-key hex.

**both-signatures**

Can you write the Bitcoin script for our shared savings? Alice and Bob must both sign a withdrawal. Neither can spend alone, and there is no recovery route or waiting period.

Keys: A = Alice, B = Bob.

Faulty draft:

```text
OP_IF A OP_CHECKSIG OP_ELSE B OP_CHECKSIG OP_ENDIF
```

Both signatures are required. The first signature check must succeed before the second check runs; a branch that accepts either signature would permit an unauthorized withdrawal.

Policy:

```text
and(pk(A),pk(B))
```

Miniscript:

```text
and_v(v:pk(A),pk(B))
```

Correct raw script (ASM):

```text
A OP_CHECKSIGVERIFY B OP_CHECKSIG
```

**either-signature**

I want a Bitcoin script that lets either Alice or Bob spend the money on their own. They should not have to contact each other or wait. Nobody else can spend.

Keys: A = Alice, B = Bob.

Faulty draft:

```text
A OP_CHECKSIGVERIFY B OP_CHECKSIG
```

Either signer is sufficient. The witness selects one branch and supplies the signature for that branch. Requiring both signatures would block the promised independent access.

Policy:

```text
or(pk(A),pk(B))
```

Miniscript:

```text
or_i(pk(A),pk(B))
```

Correct raw script (ASM):

```text
OP_IF A OP_CHECKSIG OP_ELSE B OP_CHECKSIG OP_ENDIF
```

**delay-only-recovery**

Please write a Bitcoin script for my savings. Alice can spend at any time. If needed, Bob can recover the money once the output is at least 288 blocks old from confirmation, without Alice. Alice must still be able to spend after that. Those are the only options.

Keys: A = Alice, B = Bob.

Faulty draft:

```text
288 OP_CSV OP_VERIFY OP_IF A OP_CHECKSIG OP_ELSE B OP_CHECKSIG OP_ENDIF
```

Only Bob must wait. Keep the relative lock inside the recovery branch so Alice remains unrestricted. For recovery, use a version-2-or-later transaction and a block-based input sequence of at least 288 with the relative-lock disable flag clear.

Policy:

```text
or(pk(A),and(pk(B),older(288)))
```

Miniscript:

```text
or_i(pk(A),and_v(v:older(288),pk(B)))
```

Correct raw script (ASM):

```text
OP_IF A OP_CHECKSIG OP_ELSE 288 OP_CSV OP_VERIFY B OP_CHECKSIG OP_ENDIF
```

**relative-not-absolute**

Write a Bitcoin script that lets Alice spend only after these coins have been confirmed for at least 144 blocks. The delay starts when this output confirms, not at a fixed block height. There are no other spending routes.

Keys: A = Alice.

Faulty draft:

```text
144 OP_CLTV OP_VERIFY A OP_CHECKSIG
```

Use a relative lock, not an absolute block-height lock. CSV constrains this input sequence; CLTV checks transaction locktime. Use a version-2-or-later transaction and a block-based input sequence of at least 144 with the relative-lock disable flag clear.

Policy:

```text
and(pk(A),older(144))
```

Miniscript:

```text
and_v(v:older(144),pk(A))
```

Correct raw script (ASM):

```text
144 OP_CSV OP_VERIFY A OP_CHECKSIG
```

**signature-stack**

Could you write the Bitcoin script for a payment that needs both Alice and Bob to sign? Neither signature alone should work. There are no other conditions or spending paths.

Keys: A = Alice, B = Bob.

Faulty draft:

```text
A OP_CHECKSIG B OP_CHECKSIG
```

The first check must consume its Boolean result. CHECKSIGVERIFY does that and aborts if the signature is invalid. Two consecutive CHECKSIG operations would leave the first Boolean where the next signature is needed.

Policy:

```text
and(pk(A),pk(B))
```

Miniscript:

```text
and_v(v:pk(A),pk(B))
```

Correct raw script (ASM):

```text
A OP_CHECKSIGVERIFY B OP_CHECKSIG
```

**unlisted-key**

Write the Bitcoin script for a club reserve. Any two of Alice, Bob, and Carol can authorize a withdrawal. One signature is never enough, and there must be no spending path for any other key.

Keys: A = Alice, B = Bob, C = Carol.

Faulty draft:

```text
OP_IF OP_PUSHNUM_2 A B C OP_PUSHNUM_3 OP_CHECKMULTISIG OP_ELSE UNLISTED_KEY OP_CHECKSIG OP_ENDIF
```

Remove the alternate branch for the unlisted key. The remaining script requires two of the three club signatures. Its CHECKMULTISIG witness includes the required empty dummy element before the signatures.

Policy:

```text
thresh(2,pk(A),pk(B),pk(C))
```

Miniscript:

```text
multi(2,A,B,C)
```

Correct raw script (ASM):

```text
OP_PUSHNUM_2 A B C OP_PUSHNUM_3 OP_CHECKMULTISIG
```

Verification: 12 correct answers pass strict grading; all 12 faulty drafts fail. Six independent policy/body checks, six ASM/hex round trips and six execution cross-checks pass. All 24 SFT completion masks and tokenizer round trips pass.

These are authored training examples, not an independent benchmark. The interpreter checks assume valid signatures; they do not establish full signed-transaction correctness.

Relative lock behavior follows [BIP-112](https://github.com/bitcoin/bips/blob/master/bip-0112.mediawiki). The full keys, prompts, drafts, policies, Miniscript, ASM and hex are in `script-repairs-v1-review.json`.
