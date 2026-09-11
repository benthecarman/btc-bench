# Conditioned-team examples

Names and H256/H160 replace concrete keys and digests for display. The actual training and evaluation fixtures contain the full values.

## train 192: person, person, pair, secret_pair

For our business reserve, the spending rule is at least 2 approvals out of these 4.

1. Casey can give one approval with a signature.
2. Ben can give one approval with a signature.
3. Tess and Uma share one approval and must both sign for it.
4. Cleo and Pia share one approval and must both sign for it; their joint approval also needs the 32-byte secret matching SHA-256 H256.

Count each listed approval once, even if it has several signatures or a secret. Extra approvals are fine. Do not add another way to spend. Write the Bitcoin script.

Policy:

```text
thresh(2,pk(Casey),pk(Ben),and(pk(Tess),pk(Uma)),and(and(pk(Cleo),pk(Pia)),sha256(H256)))
```

Verified Miniscript:

```text
thresh(2,and_b(pk(Tess),s:pk(Uma)),s:pk(Ben),s:pk(Casey),ajc:and_v(v:sha256(H256),and_v(v:pk(Cleo),pk_k(Pia))))
```

Chat final answer:

P2WSH witness script:

```text
Tess OP_CHECKSIG OP_SWAP Uma OP_CHECKSIG OP_BOOLAND OP_SWAP Ben OP_CHECKSIG OP_ADD OP_SWAP Casey OP_CHECKSIG OP_ADD OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF OP_SIZE 20 OP_EQUALVERIFY OP_SHA256 H256 OP_EQUALVERIFY Cleo OP_CHECKSIGVERIFY Pia OP_CHECKSIG OP_ENDIF OP_FROMALTSTACK OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## train 193: person, either, delayed_pair, sha

Our community fund needs at least 2 of the following 4 approvals for a withdrawal.

1. Owen can give one approval with a signature.
2. Ada and Nora share one approval, which either person can give alone; both signing still counts once.
3. Mina and Eli share one approval and must both sign for it; their joint approval also needs a relative time lock of at least 1024 seconds, using 2 units of 512 seconds on Bitcoin's relative-lock clock.
4. Cleo gives one approval with a signature and the 32-byte secret matching SHA-256 H256; neither item alone is enough.

Count each listed approval once, even if it has several signatures or a secret. Extra approvals are fine. Do not add another way to spend. Write the Bitcoin script.

Policy:

```text
thresh(2,pk(Owen),or(pk(Ada),pk(Nora)),and(and(pk(Mina),pk(Eli)),older(4194306)),and(pk(Cleo),sha256(H256)))
```

Verified Miniscript:

```text
thresh(2,or_b(pk(Ada),s:pk(Nora)),s:pk(Owen),ajc:and_v(v:older(4194306),and_v(v:pk(Mina),pk_k(Eli))),aj:and_v(v:pk(Cleo),sha256(H256)))
```

Chat final answer:

P2WSH witness script:

```text
Ada OP_CHECKSIG OP_SWAP Nora OP_CHECKSIG OP_BOOLOR OP_SWAP Owen OP_CHECKSIG OP_ADD OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF 4194306 OP_CSV OP_VERIFY Mina OP_CHECKSIGVERIFY Eli OP_CHECKSIG OP_ENDIF OP_FROMALTSTACK OP_ADD OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF Cleo OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_SHA256 H256 OP_EQUAL OP_ENDIF OP_FROMALTSTACK OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## train 194: secret_pair, person, hash160, person

Our business reserve needs at least 2 of the following 4 approvals for a withdrawal.

1. Eli and Ada share one approval and must both sign for it; their joint approval also needs the 32-byte secret matching SHA-256 H256.
2. Pia can give one approval with a signature.
3. Uma gives one approval with a signature and the 32-byte secret matching HASH160 H160; neither item alone is enough.
4. Alex can give one approval with a signature.

Count each listed approval once, even if it has several signatures or a secret. Extra approvals are fine. Do not add another way to spend. Write the Bitcoin script.

Policy:

```text
thresh(2,and(and(pk(Eli),pk(Ada)),sha256(H256)),pk(Pia),and(pk(Uma),hash160(H160)),pk(Alex))
```

Verified Miniscript:

```text
thresh(2,jc:and_v(v:sha256(H256),and_v(v:pk(Eli),pk_k(Ada))),s:pk(Pia),aj:and_v(v:pk(Uma),hash160(H160)),s:pk(Alex))
```

Chat final answer:

P2WSH witness script:

```text
OP_SIZE OP_0NOTEQUAL OP_IF OP_SIZE 20 OP_EQUALVERIFY OP_SHA256 H256 OP_EQUALVERIFY Eli OP_CHECKSIGVERIFY Ada OP_CHECKSIG OP_ENDIF OP_SWAP Pia OP_CHECKSIG OP_ADD OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF Uma OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_HASH160 H160 OP_EQUAL OP_ENDIF OP_FROMALTSTACK OP_ADD OP_SWAP Alex OP_CHECKSIG OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## fresh 0: person, person, hash160, delayed_pair

Can you write the Bitcoin script for our club reserve? We need at least 3 approvals to release the money.

- Ben can give one approval with a signature.
- Vik can give one approval with a signature.
- Wren gives one approval with a signature and the 32-byte secret matching HASH160 H160; neither item alone is enough.
- Owen and Uma share one approval and must both sign for it; their joint approval also needs the output to be at least 144 blocks old, measured from confirmation.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

Policy:

```text
thresh(3,pk(Ben),pk(Vik),and(pk(Wren),hash160(H160)),and(and(pk(Owen),pk(Uma)),older(144)))
```

Verified Miniscript:

```text
thresh(3,j:and_v(v:pk(Wren),hash160(H160)),s:pk(Vik),s:pk(Ben),ajc:and_v(v:older(144),and_v(v:pk(Owen),pk_k(Uma))))
```

## fresh 24: secret_pair, delayed_pair, either

Can you write the Bitcoin script for our club reserve? We need at least 2 approvals to release the money.

- Sam and Ada share one approval and must both sign for it; their joint approval also needs the 32-byte secret matching SHA-256 H256.
- Blair and Nora share one approval and must both sign for it; their joint approval also needs a relative time lock of at least 2048 seconds, using 4 units of 512 seconds on Bitcoin's relative-lock clock.
- Wren and Cleo share one approval, which either person can give alone; both signing still counts once.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

Policy:

```text
thresh(2,and(and(pk(Sam),pk(Ada)),sha256(H256)),and(and(pk(Blair),pk(Nora)),older(4194308)),or(pk(Wren),pk(Cleo)))
```

Verified Miniscript:

```text
thresh(2,jc:and_v(v:sha256(H256),and_v(v:pk(Sam),pk_k(Ada))),ajc:and_v(v:older(4194308),and_v(v:pk(Blair),pk_k(Nora))),a:or_b(pk(Wren),s:pk(Cleo)))
```

## fresh 25: delayed_pair, either, delayed_pair

I am setting up a charity fund. A withdrawal should work with any 2 of these approvals. Please write the Bitcoin script.

- Vik and Mina share one approval and must both sign for it; their joint approval also needs a relative time lock of at least 3072 seconds, using 6 units of 512 seconds on Bitcoin's relative-lock clock.
- Tess and Owen share one approval, which either person can give alone; both signing still counts once.
- Nora and Pia share one approval and must both sign for it; their joint approval also needs a relative time lock of at least 6144 seconds, using 12 units of 512 seconds on Bitcoin's relative-lock clock.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

Policy:

```text
thresh(2,and(and(pk(Vik),pk(Mina)),older(4194310)),or(pk(Tess),pk(Owen)),and(and(pk(Nora),pk(Pia)),older(4194316)))
```

Verified Miniscript:

```text
thresh(2,jc:and_v(v:older(4194310),and_v(v:pk(Vik),pk_k(Mina))),a:or_b(pk(Tess),s:pk(Owen)),ajc:and_v(v:older(4194316),and_v(v:pk(Nora),pk_k(Pia))))
```
