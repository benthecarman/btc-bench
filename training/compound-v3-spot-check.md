# Compound v3 spot checks

These are saved chat answers from the development-selected checkpoint 144. No answers were repaired. Key labels below abbreviate exact public keys only in the displayed policy and Miniscript. The original bytes remain in the run files.

## Correct composition of two conditioned teams

`t1-human-composed-val-compound-v3-024`

The policy, Miniscript, and final script all pass their separate checks.

### Request

Can you write the Bitcoin script for our club reserve? We need at least 2 approvals to release the money.

- Sam and Ada share one approval and must both sign for it; their joint approval also needs the 32-byte secret matching SHA-256 8e3c624f1987d9f0aefdc3e8f6c3321ac8a24a8c764a093d28a69cbca237bbe8.
- Blair and Nora share one approval and must both sign for it; their joint approval also needs a relative time lock of at least 2048 seconds, using 4 units of 512 seconds on Bitcoin's relative-lock clock.
- Wren and Cleo share one approval, which either person can give alone; both signing still counts once.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

### Reference policy

```text
thresh(2,and(and(pk(Sam),pk(Ada)),sha256(8e3c624f1987d9f0aefdc3e8f6c3321ac8a24a8c764a093d28a69cbca237bbe8)),and(and(pk(Blair),pk(Nora)),older(4194308)),or(pk(Wren),pk(Cleo)))
```

### Model policy

```text
thresh(2,and(and(pk(Sam),pk(Ada)),sha256(8e3c624f1987d9f0aefdc3e8f6c3321ac8a24a8c764a093d28a69cbca237bbe8)),and(and(pk(Blair),pk(Nora)),older(4194308)),or(pk(Wren),pk(Cleo)))
```

### Model Miniscript

```text
thresh(2,jc:and_v(v:sha256(8e3c624f1987d9f0aefdc3e8f6c3321ac8a24a8c764a093d28a69cbca237bbe8),and_v(v:pk(Sam),pk_k(Ada))),ajc:and_v(v:older(4194308),and_v(v:pk(Blair),pk_k(Nora))),a:or_b(pk(Wren),s:pk(Cleo)))
```

### Checks

```json
{
  "final": {
    "task_id": "t1-human-composed-val-compound-v3-024",
    "score": 1.0,
    "size_score": null,
    "reason": null
  },
  "intermediate": {
    "id": "selected/chat/t1-human-composed-val-compound-v3-024",
    "miniscript": {
      "reason": "semantically equivalent",
      "status": "equivalent"
    },
    "policy": {
      "reason": "semantically equivalent",
      "status": "equivalent"
    }
  }
}
```

### Original final answer

P2WSH witness script:

```text
OP_SIZE OP_0NOTEQUAL OP_IF OP_SIZE 20 OP_EQUALVERIFY OP_SHA256 8e3c624f1987d9f0aefdc3e8f6c3321ac8a24a8c764a093d28a69cbca237bbe8 OP_EQUALVERIFY 03ca4e98f8943f0164e2a15b9fce43f7ff61c13d638f2d1b61406a893aab6878e8 OP_CHECKSIGVERIFY 034843acf3c5298dcdf9074470584451a2ecceee28a132fa0538065851bb575c48 OP_CHECKSIG OP_ENDIF OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF 4194308 OP_CSV OP_VERIFY 034401f395ef27a7a3d1f5c6096baca79d2eb9b04305f633090ba2c932d4988410 OP_CHECKSIGVERIFY 0366b19b745feae882e956eb9235e6bd51a659efcd099e106687910d2b2d989ebd OP_CHECKSIG OP_ENDIF OP_FROMALTSTACK OP_ADD OP_TOALTSTACK 0212bc10ebeee66bc761661d699b2e477a1f559367d7fdc73b3f6d51ea04d7f891 OP_CHECKSIG OP_SWAP 039b6e930032f8d4c5503d87678d02d9500679c6eedcaa88abf45e011b50856dea OP_CHECKSIG OP_BOOLOR OP_FROMALTSTACK OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## Delay lost during script construction

`t1-human-composed-val-compound-v3-020`

The policy includes the 48-block delay on Cleo and Nora. The Miniscript and final script omit it. Before 48 blocks, Cleo and Nora plus Eli can satisfy two approvals in the candidate; the request permits only one.

### Request

I am setting up a community fund. A withdrawal should work with any 2 of these approvals. Please write the Bitcoin script.

- Casey gives one approval with a signature, but it can count only with the output to be at least 144 blocks old, measured from confirmation.
- Cleo and Nora share one approval and must both sign for it; their joint approval also needs the output to be at least 48 blocks old, measured from confirmation.
- Eli and Wren share one approval, which either person can give alone; both signing still counts once.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

### Reference policy

```text
thresh(2,and(pk(Casey),older(144)),and(and(pk(Cleo),pk(Nora)),older(48)),or(pk(Eli),pk(Wren)))
```

### Model policy

```text
thresh(2,and(pk(Casey),older(144)),and(and(pk(Cleo),pk(Nora)),older(48)),or(pk(Eli),pk(Wren)))
```

### Model Miniscript

```text
thresh(2,jc:and_v(v:pk(Cleo),pk_k(Nora)),snj:and_v(v:pk(Casey),older(144)),a:or_b(pk(Eli),s:pk(Wren)))
```

### Checks

```json
{
  "final": {
    "task_id": "t1-human-composed-val-compound-v3-020",
    "score": 0.0,
    "size_score": null,
    "reason": "the script is valid but not semantically equivalent to the required spending policy",
    "failure": "wrong semantics"
  },
  "intermediate": {
    "id": "selected/chat/t1-human-composed-val-compound-v3-020",
    "miniscript": {
      "reason": "the script is valid but not semantically equivalent to the required spending policy",
      "status": "not equivalent"
    },
    "policy": {
      "reason": "semantically equivalent",
      "status": "equivalent"
    }
  }
}
```

### Original final answer

P2WSH witness script:

```text
0314d24c9153a52ec7c4d3f01c534033689b429b65f60c554ac6b83d7cedb43019 OP_CHECKSIG OP_SWAP 03ca39ccc1b88c70669a4154affecb06920910cb379fe2f43a53459a28120387df OP_CHECKSIG OP_BOOLAND OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF 02f4dd5fec3262cf29eabacff4663e0021624c14c2c44529ced5b707ad696a67e0 OP_CHECKSIGVERIFY 144 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_ADD OP_TOALTSTACK 03bf87c089fcd069d22eb29e971b1b4cc158116320d36794620ac8f63ff9fc2209 OP_CHECKSIG OP_SWAP 0294ba17067549430f009fa692d0b9999b7b9e3ee05d29d9f43dcb580122cf3479 OP_CHECKSIG OP_BOOLOR OP_FROMALTSTACK OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## Correct final script with invalid Miniscript

`t1-human-composed-val-compound-v3-000`

The stated Miniscript adds c: to a B-type hashlock fragment, which fails type checking. The final script matches the required spending policy. A passing final answer does not establish a valid derivation.

### Request

Can you write the Bitcoin script for our club reserve? We need at least 3 approvals to release the money.

- Ben can give one approval with a signature.
- Vik can give one approval with a signature.
- Wren gives one approval with a signature and the 32-byte secret matching HASH160 117c737c18ae64901775ca956dbcfc93a01dd081; neither item alone is enough.
- Owen and Uma share one approval and must both sign for it; their joint approval also needs the output to be at least 144 blocks old, measured from confirmation.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

### Reference policy

```text
thresh(3,pk(Ben),pk(Vik),and(pk(Wren),hash160(117c737c18ae64901775ca956dbcfc93a01dd081)),and(and(pk(Owen),pk(Uma)),older(144)))
```

### Model policy

```text
thresh(3,pk(Ben),pk(Vik),and(pk(Wren),hash160(117c737c18ae64901775ca956dbcfc93a01dd081)),and(and(pk(Owen),pk(Uma)),older(144)))
```

### Model Miniscript

```text
thresh(3,jc:and_v(v:pk(Wren),hash160(117c737c18ae64901775ca956dbcfc93a01dd081)),s:pk(Vik),s:pk(Ben),ajc:and_v(v:older(144),and_v(v:pk(Owen),pk_k(Uma))))
```

### Checks

```json
{
  "final": {
    "task_id": "t1-human-composed-val-compound-v3-000",
    "score": 1.0,
    "size_score": null,
    "reason": null
  },
  "intermediate": {
    "id": "selected/chat/t1-human-composed-val-compound-v3-000",
    "miniscript": {
      "reason": "typecheck: fragment \u00abc:and_v(v:pk(032b3952999c3c1f86b2c475b182a8dec245598a66bd94fcdc6373a4ad838ff6fe),hash160(117c737c18ae64901775ca956dbcfc93a01dd081))\u00bb cannot wrap a fragment of type B",
      "status": "invalid"
    },
    "policy": {
      "reason": "semantically equivalent",
      "status": "equivalent"
    }
  }
}
```

### Original final answer

P2WSH witness script:

```text
OP_SIZE OP_0NOTEQUAL OP_IF 032b3952999c3c1f86b2c475b182a8dec245598a66bd94fcdc6373a4ad838ff6fe OP_CHECKSIGVERIFY OP_SIZE 20 OP_EQUALVERIFY OP_HASH160 117c737c18ae64901775ca956dbcfc93a01dd081 OP_EQUAL OP_ENDIF OP_SWAP 03277fd75e57c1a7a2d0bc6ca01776e6a55e68b8edb2d0d052dd84856a3e5c95fa OP_CHECKSIG OP_ADD OP_SWAP 02211f6ce71acd7f3ce63b732051afbe64746c3803c3c6258ffbe856764282ab9a OP_CHECKSIG OP_ADD OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF 144 OP_CSV OP_VERIFY 02bf0ea4b689bd3fcbfac3d0cb2bc4c1306aa179db62ac04d4e9ecdcec1510819a OP_CHECKSIGVERIFY 03b4d5717c58c79727630f5b88e9f5221093b812002e6c1711e81f746e19939745 OP_CHECKSIG OP_ENDIF OP_FROMALTSTACK OP_ADD OP_PUSHNUM_3 OP_EQUAL
```
