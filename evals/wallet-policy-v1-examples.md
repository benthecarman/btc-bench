# Wallet-policy v1 spot checks

Four fixed scenarios with their reference expressions and saved model answers. These examples are for review; they are not a random sample. The full model prompts also specify the output contract and key mapping.

## wp-native-quorum-0

Our club wants a native SegWit P2WSH wallet. Any two of Alice, Bob, and Cleo should be able to approve a withdrawal. Allow exactly these spending routes.

Key mapping:

@0 = Alice, @1 = Bob, @2 = Cleo

Separately authored spending policy:

```text
thresh(2,pk(@0),pk(@1),pk(@2))
```

Reference descriptor template:

```text
wsh(multi(2,@0/**,@1/**,@2/**))
```

Canonical BIP-388 English:

> Any 2 of @0, @1 and @2 must sign

| Interface | Template | Concrete |
|---|---|---|
| chat | failed | failed |
| submit | failed | failed |

Saved chat template answer:



Bitcoin script:

```text
@0 OP_CHECKSIG OP_SWAP @1 OP_CHECKSIG OP_SWAP @2 OP_CHECKSIG OP_BOOLOR OP_BOOLOR OP_PUSHNUM_2 OP_EQUAL
```

Grader reason: `UnrecognizedFragment`


Saved chat concrete answer:



Bitcoin descriptor:

```text
multi(2,020a4df1caa0bc305db8db8998531c130d3f7ac57d55ae9a6cc8646b394ae782c0,03f55a2515de165d2f2d6f3e8c8d5740c1bbf4fa909ff94d865dceb02f2b611e02,033a7ffa828d532110b51a1a7dd33dc44b41e91283fad869d722e841225230c986) OP_CHECKMULTISIGVERIFY 020a4df1caa0bc305db8db8998531c130d3f7ac57d55ae9a6cc8646b394ae782c0 OP_CHECKSIG
```

Grader reason: `trailing data ` ...` (position 209)`

## wp-recovery-delay-1

For this Taproot wallet, Cleo is the owner and can spend without waiting. Dara is the backup: her signature works only when these coins have aged at least 1008 blocks since confirmation. Allow exactly these spending routes.

Key mapping:

@0 = Cleo, @1 = Dara

Separately authored spending policy:

```text
or(pk(@0),and(pk(@1),older(1008)))
```

Reference descriptor template:

```text
tr(@0/**,and_v(v:pk(@1/**),older(1008)))
```

Canonical BIP-388 English:

> Main path: spendable by @0
> @1 must sign, 1008 blocks after receiving

| Interface | Template | Concrete |
|---|---|---|
| chat | failed | failed |
| submit | failed | failed |

Saved chat template answer:



Taproot descriptor:

```text
@0 || @1 older 1008
```

Grader reason: `UnrecognizedFragment`


Saved chat concrete answer:



Taproot descriptor:

```text
or(pk(9975dc0b068e092c052995872797244f90d4e959118fbc7bb54c36feebee9e7c),and_v(v:pk(b555aa8938a03d83395285160e8af04961cea7a1f6dff89fc37a157d95c35f14),older(1008)))
```

Grader reason: `unrecognized name 'or'`

## wp-joint-councils-0

Set up a Taproot reserve. Alice has an immediate owner route. The alternative route needs both council approvals: at least two of Bob, Cleo, and Dara, and at least one of Eli and Finn. Extra signatures in one council cannot replace the other council approval. The councils do not need Alice to sign. Allow exactly these spending routes.

Key mapping:

@0 = Alice, @1 = Bob, @2 = Cleo, @3 = Dara, @4 = Eli, @5 = Finn

Separately authored spending policy:

```text
or(pk(@0),and(thresh(2,pk(@1),pk(@2),pk(@3)),thresh(1,pk(@4),pk(@5))))
```

Reference descriptor template:

```text
tr(@0/**,and_v(v:multi_a(2,@1/**,@2/**,@3/**),multi_a(1,@4/**,@5/**)))
```

Canonical BIP-388 English:

> Main path: spendable by @0
> Any 2 of @1, @2 and @3 must sign - and also - any 1 of @4 and @5 must sign

| Interface | Template | Concrete |
|---|---|---|
| chat | failed | failed |
| submit | failed | failed |

Saved chat template answer:



Taproot descriptor:

```text
@0||and_v(v:multi_a(2,@1,@2,@3),and_v(v:pk(@4),pk(@5)))
```

Grader reason: `UnrecognizedFragment`


Saved chat concrete answer:



Taproot descriptor:

```text
f3187214c52aae921c62a70f304798acebbd95ae826035b13c00242b76c08c9e||and_v(v:multi_a(2,5659dbceb11dd77f8061b1c17d8430f5d85911d4d82288751e725e7e5d657272,ec3abe252c80d1350aed9599dfada023547cdf08a452dc17eaa272dbe89b6de0,a5ef5963a17ed6fb94f692a646deb7846fab06ed3596b274fbfb9127e953a5f1),and_v(v:pk(bc4d9a21e29e06cec7905f8e495079004885106988d8bbba9e677bc6a740d915),pk(7e7fef9f1f809c5f4f4db03f73e72192aba6495c4c88883a50e9678d225399c4)))
```

Grader reason: `Multi a(CHECKSIGADD) only allowed post tapscript`

## wp-alternative-recovery-0

Make a Taproot wallet with three alternatives. Mina alone can spend immediately. Nora alone can also spend immediately. Otherwise, any two of Omar, Paz, and Quinn can spend after the output has aged at least 288 blocks from confirmation. The delayed team needs neither Mina nor Nora. Allow exactly these spending routes.

Key mapping:

@0 = Mina, @1 = Nora, @2 = Omar, @3 = Paz, @4 = Quinn

Separately authored spending policy:

```text
or(pk(@0),or(pk(@1),and(thresh(2,pk(@2),pk(@3),pk(@4)),older(288))))
```

Reference descriptor template:

```text
tr(@0/**,{pk(@1/**),and_v(v:multi_a(2,@2/**,@3/**,@4/**),older(288))})
```

Canonical BIP-388 English:

> Main path: spendable by @0
> @1 must sign
> Any 2 of @2, @3 and @4 must sign, 288 blocks after receiving

| Interface | Template | Concrete |
|---|---|---|
| chat | failed | failed |
| submit | failed | failed |

Saved chat template answer:



taproot:@0||@1||multi_a(2,@2,@3,@4,older(288))

Grader reason: `UnrecognizedFragment`


Saved chat concrete answer:



Taproot descriptor:

```text
3321f0611a1b32e4a5086e0c6ce906a3546afd11e165c55953834d1d88252986||466f1e5c6fa28e678a08e8c92a3fb90f151b1421a2317ab32fb3ba46919b2285||and_v(v:multi_a(2,a338f64dc0d1714bf093ae17f705445b9df6c58783e3d7d07541685c9d5f6835,2b3d4279e10787ffcf265c5ce5ad434a78b1c3a3fecbcecfc0d88fb94a3926bc,2da1814b48358f13be34476a1a271fc78976a0bfe05e001b6fd0a38c9edada1f),older(288))
```

Grader reason: `Multi a(CHECKSIGADD) only allowed post tapscript`
