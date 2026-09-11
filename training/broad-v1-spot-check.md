# Broad SFT spot checks

These are saved evaluation answers from checkpoint 528 and its parent. The reserved examples were inspected after checkpoint selection. No answer was repaired. Public keys below are replaced with their request labels for readability; these labelled examples are not runnable. The exact prompts, answers, and references are saved in `runs/broad-v1-spot-check.json`.

## A complete script success

`t1-human-composed-val-broad-v1-two-locked-departments-007-write`

The selected model passes in both interfaces. Its stated policy and Miniscript also pass in both. It keeps the relative clock on the first council and the absolute clock on the second.

Request:

Can you write the Bitcoin script for this reserve?

I need approval from two separate councils. For the first council, at least 2 of Jules and Cleo sign, and the output meets a relative time lock of 2048 seconds, using 4 units of 512 seconds on Bitcoin's relative-lock clock. For the second, at least 3 of Luz, Ari, Hugo, and Kai sign, and the spending transaction has time-based locktime (Unix seconds) of at least 1867782019. Both council approvals are required for the same withdrawal. Neither council can waive the other council's clock requirement.

Allow every spend that meets these rules and no other route.

Reference policy:

```text
and(and(and(pk(Jules),pk(Cleo)),older(4194308)),and(thresh(3,pk(Luz),pk(Ari),pk(Hugo),pk(Kai)),after(1867782019)))
```

Reference Miniscript:

```text
and_v(v:and_v(v:and_v(v:pk(Jules),pk(Cleo)),older(4194308)),and_v(v:multi(3,Luz,Ari,Hugo,Kai),after(1867782019)))
```

| Model | Interface | Final answer | Stated policy | Stated Miniscript |
|---|---|---|---|---|
| parent | chat | decode reject | not equivalent | not equivalent |
| parent | submit | decode reject | not equivalent | not equivalent |
| step528 | chat | correct | equivalent | equivalent |
| step528 | submit | correct | equivalent | equivalent |

Selected chat policy:

```text
and(and(and(pk(Jules),pk(Cleo)),older(4194308)),and(thresh(3,pk(Luz),pk(Ari),pk(Hugo),pk(Kai)),after(1867782019)))
```

Selected chat miniscript:

```text
and_v(v:and_v(v:and_v(v:pk(Jules),pk(Cleo)),older(4194308)),and_v(v:multi(3,Luz,Ari,Hugo,Kai),after(1867782019)))
```

Selected chat final answer:



P2WSH witness script:

```text
Jules OP_CHECKSIGVERIFY Cleo OP_CHECKSIGVERIFY 4194308 OP_CSV OP_VERIFY OP_PUSHNUM_3 Luz Ari Hugo Kai OP_PUSHNUM_4 OP_CHECKMULTISIGVERIFY 1867782019 OP_CLTV
```

## A valid descriptor with a different expression

`t4-human-composed-val-broad-v1-alternative-evidence-bundles-005-tree`

The selected descriptor passes in both interfaces. The chat answer uses or_i where the reference uses andor. The oracle accepts their equivalent spending behavior. The owner route remains separate from both evidence bundles.

Request:

Please give me a Taproot tr() descriptor for our wallet. Dara signs to spend immediately without any other approval, secret, or waiting period. The rules in the next paragraph apply only to the alternative spending arrangement, not to Dara's route:

The signers must satisfy this rule: at least 2 of Hugo, Kai, and Bea sign. They must also supply one complete evidence bundle. Bundle one requires that Jules signs, the 32-byte secret matching SHA-256 1ec9e358632a2fbc20f6b6fabcff81bc277f3d2d2ba26bc6870bb45fcec58daa is supplied, and the output is at least 144 blocks old, measured from confirmation. Bundle two requires that Luz signs, the 32-byte secret matching HASH160 398edff797c3b0813a561672231b541c7b528cdd is supplied, and the spending transaction has block-height locktime of at least 934420. Either complete bundle works; do not mix the signature or secret from one with the clock from the other.

The alternative arrangement does not require Dara to sign. Allow all of these routes and no others. Choose the internal key and leaves to reduce the worst-case input weight.

Reference policy:

```text
or(pk(Dara),and(thresh(2,pk(Hugo),pk(Kai),pk(Bea)),or(and(pk(Jules),and(sha256(1ec9e358632a2fbc20f6b6fabcff81bc277f3d2d2ba26bc6870bb45fcec58daa),older(144))),and(pk(Luz),and(hash160(398edff797c3b0813a561672231b541c7b528cdd),after(934420))))))
```

Reference descriptor:

```text
tr(Dara,and_v(v:multi_a(2,Hugo,Kai,Bea),andor(pk(Jules),and_v(v:sha256(1ec9e358632a2fbc20f6b6fabcff81bc277f3d2d2ba26bc6870bb45fcec58daa),older(144)),and_v(v:pk(Luz),and_v(v:hash160(398edff797c3b0813a561672231b541c7b528cdd),after(934420))))))
```

| Model | Interface | Final answer | Stated policy | Stated Miniscript |
|---|---|---|---|---|
| parent | chat | wrong semantics | not checked | not checked |
| parent | submit | wrong semantics | not checked | not checked |
| step528 | chat | correct | not checked | not checked |
| step528 | submit | correct | not checked | not checked |

Selected chat policy:

```text
or(pk(Dara),and(thresh(2,pk(Hugo),pk(Kai),pk(Bea)),or(and(and(pk(Jules),sha256(1ec9e358632a2fbc20f6b6fabcff81bc277f3d2d2ba26bc6870bb45fcec58daa)),older(144)),and(pk(Luz),and(hash160(398edff797c3b0813a561672231b541c7b528cdd),after(934420)))))
```

Selected chat final answer:



Taproot descriptor:

```text
tr(Dara,and_v(v:multi_a(2,Hugo,Kai,Bea),or_i(and_v(v:and_v(v:pk(Jules),sha256(1ec9e358632a2fbc20f6b6fabcff81bc277f3d2d2ba26bc6870bb45fcec58daa)),older(144)),and_v(v:pk(Luz),and_v(v:hash160(398edff797c3b0813a561672231b541c7b528cdd),after(934420))))))
```

## A correct final script with invalid stated Miniscript

`t1-human-composed-val-broad-v1-three-conditioned-departments-006-write`

The final script and policy pass in both interfaces, but the stated Miniscript fails type checking in both. The c wrapper is applied to a B fragment. The correct final script does not establish that the displayed derivation is valid.

Request:

Can you write the Bitcoin script for this reserve?

Our three departments each get one vote. Any two department votes release the funds. Their votes are defined below:
- Department 1: at least 2 of Luz and Ari sign, and the 32-byte secret matching HASH160 7724070d697170e85f194477ae6a17e22f0340b8 is supplied.
- Department 2: at least 2 of Inez, Bea, and Finn sign, and the output is at least 24 blocks old, measured from confirmation.
- Department 3: at least 2 of Eli and Kai sign, and the output is at least 4 blocks old, measured from confirmation.
Signatures inside a department do not count as extra votes.

Allow every spend that meets these rules and no other route.

Reference policy:

```text
thresh(2,and(and(pk(Luz),pk(Ari)),hash160(7724070d697170e85f194477ae6a17e22f0340b8)),and(thresh(2,pk(Inez),pk(Bea),pk(Finn)),older(24)),and(and(pk(Eli),pk(Kai)),older(4)))
```

Reference Miniscript:

```text
thresh(2,jc:and_v(v:hash160(7724070d697170e85f194477ae6a17e22f0340b8),and_v(v:pk(Luz),pk_k(Ari))),anj:and_v(v:multi(2,Inez,Bea,Finn),older(24)),ajc:and_v(v:older(4),and_v(v:pk(Eli),pk_k(Kai))))
```

| Model | Interface | Final answer | Stated policy | Stated Miniscript |
|---|---|---|---|---|
| parent | chat | decode reject | equivalent | invalid |
| parent | submit | decode reject | missing | invalid |
| step528 | chat | correct | equivalent | invalid |
| step528 | submit | correct | equivalent | invalid |

Selected chat policy:

```text
thresh(2,and(and(pk(Luz),pk(Ari)),hash160(7724070d697170e85f194477ae6a17e22f0340b8)),and(thresh(2,pk(Inez),pk(Bea),pk(Finn)),older(24)),and(and(pk(Eli),pk(Kai)),older(4)))
```

Selected chat miniscript:

```text
thresh(2,j:and_v(v:and_v(v:pk(Luz),pk(Ari)),hash160(7724070d697170e85f194477ae6a17e22f0340b8)),anj:and_v(v:multi(2,Inez,Bea,Finn),older(24)),ajc:and_v(v:and_v(v:pk(Eli),pk(Kai)),older(4)))
```

Selected chat final answer:



P2WSH witness script:

```text
OP_SIZE OP_0NOTEQUAL OP_IF Luz OP_CHECKSIGVERIFY Ari OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_HASH160 7724070d697170e85f194477ae6a17e22f0340b8 OP_EQUAL OP_ENDIF OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF OP_PUSHNUM_2 Inez Bea Finn OP_PUSHNUM_3 OP_CHECKMULTISIGVERIFY 24 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_FROMALTSTACK OP_ADD OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF Eli OP_CHECKSIGVERIFY Kai OP_CHECKSIGVERIFY 4 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_FROMALTSTACK OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

Miniscript check:

```text
typecheck: fragment «c:and_v(v:and_v(v:pk(Eli),pk(Kai)),older(4))» cannot wrap a fragment of type B
```

## A correct stated policy followed by a rejected script

`t1-human-composed-val-broad-v1-alternative-evidence-bundles-000-write`

In chat, the policy is equivalent, but the stated Miniscript is invalid in Segwit v0 and the final script is rejected. The expression mixes a multi_a fragment with compressed keys and changes the arrangement of the required approvals.

Request:

I need a Bitcoin script with the following spending rules.

The signers must satisfy this rule: at least 2 of Dara and Hugo sign. They must also supply one complete evidence bundle. Bundle one requires that Eli signs, the 32-byte secret matching SHA-256 f710ca062db2f5d92e52c74d8683e8353c7a72c82d04a4644a17af13a3c68f8e is supplied, and the output is at least 24 blocks old, measured from confirmation. Bundle two requires that Kai signs, the 32-byte secret matching HASH160 49bcf61c9baa5390e106b0d0cc8a373bba9e09da is supplied, and the spending transaction has time-based locktime (Unix seconds) of at least 1853901559. Either complete bundle works; do not mix the signature or secret from one with the clock from the other.

Allow every spend that meets these rules and no other route.

Reference policy:

```text
and(and(pk(Dara),pk(Hugo)),or(and(pk(Eli),and(sha256(f710ca062db2f5d92e52c74d8683e8353c7a72c82d04a4644a17af13a3c68f8e),older(24))),and(pk(Kai),and(hash160(49bcf61c9baa5390e106b0d0cc8a373bba9e09da),after(1853901559)))))
```

Reference Miniscript:

```text
and_v(v:and_v(v:pk(Dara),pk(Hugo)),andor(pk(Eli),and_v(v:sha256(f710ca062db2f5d92e52c74d8683e8353c7a72c82d04a4644a17af13a3c68f8e),older(24)),and_v(v:pk(Kai),and_v(v:hash160(49bcf61c9baa5390e106b0d0cc8a373bba9e09da),after(1853901559)))))
```

| Model | Interface | Final answer | Stated policy | Stated Miniscript |
|---|---|---|---|---|
| parent | chat | decode reject | invalid | invalid |
| parent | submit | decode reject | invalid | invalid |
| step528 | chat | decode reject | equivalent | invalid |
| step528 | submit | wrong semantics | equivalent | invalid |

Selected chat policy:

```text
and(thresh(2,pk(Dara),pk(Hugo)),or(and(and(pk(Eli),sha256(f710ca062db2f5d92e52c74d8683e8353c7a72c82d04a4644a17af13a3c68f8e)),older(24)),and(and(pk(Kai),hash160(49bcf61c9baa5390e106b0d0cc8a373bba9e09da)),after(1853901559))))
```

Selected chat miniscript:

```text
and_v(or_c(multi_a(2,Dara,Hugo),v:and_v(v:and_v(v:pk(Eli),sha256(f710ca062db2f5d92e52c74d8683e8353c7a72c82d04a4644a17af13a3c68f8e)),older(24))),and_v(v:and_v(v:pk(Kai),hash160(49bcf61c9baa5390e106b0d0cc8a373bba9e09da)),after(1853901559)))
```

Selected chat final answer:



P2WSH witness script:

```text
OP_PUSHNUM_2 Dara Hugo OP_PUSHNUM_3 OP_CHECKMULTISIG OP_NOTIF Eli OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_SHA256 f710ca062db2f5d92e52c74d8683e8353c7a72c82d04a4644a17af13a3c68f8e OP_EQUALVERIFY 24 OP_CSV OP_VERIFY OP_ENDIF Kai OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_HASH160 49bcf61c9baa5390e106b0d0cc8a373bba9e09da OP_EQUALVERIFY 1853901559 OP_CLTV
```

Miniscript check:

```text
Multi a(CHECKSIGADD) only allowed post tapscript
```

## The requested answer type is still missed in chat

`t4-human-tree-recovery`

The request explicitly asks for a tr() descriptor. The selected chat answer returns raw Bitcoin script instead. The selected submit answer to the same question is a correct descriptor. This is a remaining instruction and output-format problem.

Request:

Can you put together a tr() descriptor for my wallet? My owner key can spend immediately, or my backup key can spend once the output is 1008 blocks old. Both remain valid after that. Those are the only spending options. Please arrange it to keep the most expensive spend as small as possible.

Reference policy:

```text
or(pk(owner),and(pk(backup),older(1008)))
```

Reference descriptor:

```text
tr(owner,and_v(v:pk(backup),older(1008)))
```

| Model | Interface | Final answer | Stated policy | Stated Miniscript |
|---|---|---|---|---|
| parent | chat | parse error | not checked | not checked |
| parent | submit | correct | not checked | not checked |
| step528 | chat | parse error | not checked | not checked |
| step528 | submit | correct | not checked | not checked |

Selected chat policy:

```text
or(pk(owner),and(pk(backup),older(1008)))
```

Selected chat miniscript:

```text
or_d(pk(owner),and_v(v:pk(backup),older(1008)))
```

Selected chat final answer:



Bitcoin script:

```text
owner OP_CHECKSIG OP_IFDUP OP_NOTIF backup OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF
```
