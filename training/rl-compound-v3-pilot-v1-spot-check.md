# RL/SFT pilot spot checks

These are saved answers to three existing transfer questions. No candidate expressions or scripts were repaired. Public keys are replaced with their request labels in displayed policy and Miniscript; final answers retain their exact original text.

## Both methods fix a Miniscript wrapper

`t1-human-composed-val-compound-v3-001`

The parent chat answer has an invalid wrapper and a rejected final script. RL and additional SFT both produce equivalent Miniscript and a passing final script.

### Request

Can you write the Bitcoin script for our co-op wallet? We need at least 2 approvals to release the money.

- Blair can give one approval with a signature.
- Vik and Casey share one approval and must both sign for it; their joint approval also needs a relative time lock of at least 2048 seconds, using 4 units of 512 seconds on Bitcoin's relative-lock clock.
- Eli and Mina share one approval, which either person can give alone; both signing still counts once.
- Wren can give one approval with a signature.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

### Reference policy

```text
thresh(2,pk(Blair),and(and(pk(Vik),pk(Casey)),older(4194308)),or(pk(Eli),pk(Mina)),pk(Wren))
```

| Model | Interface | Final script | Policy | Miniscript |
|---|---|---|---|---|
| parent | chat | decode reject | equivalent | invalid |
| parent | submit | correct | equivalent | equivalent |
| rl | chat | correct | equivalent | equivalent |
| rl | submit | correct | equivalent | equivalent |
| sft | chat | correct | equivalent | equivalent |
| sft | submit | correct | equivalent | equivalent |

### parent chat answer

Policy:

```text
thresh(2,pk(Blair),and(and(pk(Vik),pk(Casey)),older(4194308)),or(pk(Eli),pk(Mina)),pk(Wren))
```

Miniscript:

```text
thresh(2,or_b(pk(Eli),s:pk(Mina)),snj:and_v(v:older(4194308),and_v(v:pk(Vik),pk_k(Casey))),s:pk(Blair),s:pk(Wren))
```

Final answer:

P2WSH witness script:

```text
03d5c8ae8a3e45f206f78564ba01095f8cd4deadad178f458abe797c4f44115e1a OP_CHECKSIG OP_SWAP 03be8d813e637c1ad2deea2ae8ceddc7052353454ed7552dabfbbf79cc280ca052 OP_CHECKSIG OP_BOOLOR OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF 4194308 OP_CSV OP_VERIFY 02c66aa3432abb947a7351b56ce0fe1b691d9c4f540fc1d605cc19175b069d8ca6 OP_CHECKSIGVERIFY 0362ad24c0761774e450d535834775d4831b9562bd0f20503cb7e6b35c77ef5971 OP_CHECKSIG OP_ENDIF OP_0NOTEQUAL OP_ADD OP_SWAP 027a4240857ff65ad45f660755e879ae5be0ab537095376c3b3e6afb0fce971d14 OP_CHECKSIG OP_ADD OP_SWAP 0236c0bf4babf3324dc5e59653b0a391155fbae83a216ddd4a13a2793fa23ee611 OP_CHECKSIG OP_ADD OP_PUSHNUM_2 OP_EQUAL
```


### rl chat answer

Policy:

```text
thresh(2,pk(Blair),and(and(pk(Vik),pk(Casey)),older(4194308)),or(pk(Eli),pk(Mina)),pk(Wren))
```

Miniscript:

```text
thresh(2,or_b(pk(Eli),s:pk(Mina)),ajc:and_v(v:older(4194308),and_v(v:pk(Vik),pk_k(Casey))),s:pk(Blair),s:pk(Wren))
```

Final answer:

P2WSH witness script:

```text
03d5c8ae8a3e45f206f78564ba01095f8cd4deadad178f458abe797c4f44115e1a OP_CHECKSIG OP_SWAP 03be8d813e637c1ad2deea2ae8ceddc7052353454ed7552dabfbbf79cc280ca052 OP_CHECKSIG OP_BOOLOR OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF 4194308 OP_CSV OP_VERIFY 02c66aa3432abb947a7351b56ce0fe1b691d9c4f540fc1d605cc19175b069d8ca6 OP_CHECKSIGVERIFY 0362ad24c0761774e450d535834775d4831b9562bd0f20503cb7e6b35c77ef5971 OP_CHECKSIG OP_ENDIF OP_FROMALTSTACK OP_ADD OP_SWAP 027a4240857ff65ad45f660755e879ae5be0ab537095376c3b3e6afb0fce971d14 OP_CHECKSIG OP_ADD OP_SWAP 0236c0bf4babf3324dc5e59653b0a391155fbae83a216ddd4a13a2793fa23ee611 OP_CHECKSIG OP_ADD OP_PUSHNUM_2 OP_EQUAL
```


### sft chat answer

Policy:

```text
thresh(2,pk(Blair),and(and(pk(Vik),pk(Casey)),older(4194308)),or(pk(Eli),pk(Mina)),pk(Wren))
```

Miniscript:

```text
thresh(2,jc:and_v(v:older(4194308),and_v(v:pk(Vik),pk_k(Casey))),s:pk(Blair),a:or_b(pk(Eli),s:pk(Mina)),s:pk(Wren))
```

Final answer:

P2WSH witness script:

```text
OP_SIZE OP_0NOTEQUAL OP_IF 4194308 OP_CSV OP_VERIFY 02c66aa3432abb947a7351b56ce0fe1b691d9c4f540fc1d605cc19175b069d8ca6 OP_CHECKSIGVERIFY 0362ad24c0761774e450d535834775d4831b9562bd0f20503cb7e6b35c77ef5971 OP_CHECKSIG OP_ENDIF OP_SWAP 027a4240857ff65ad45f660755e879ae5be0ab537095376c3b3e6afb0fce971d14 OP_CHECKSIG OP_ADD OP_TOALTSTACK 03d5c8ae8a3e45f206f78564ba01095f8cd4deadad178f458abe797c4f44115e1a OP_CHECKSIG OP_SWAP 03be8d813e637c1ad2deea2ae8ceddc7052353454ed7552dabfbbf79cc280ca052 OP_CHECKSIG OP_BOOLOR OP_FROMALTSTACK OP_ADD OP_SWAP 0236c0bf4babf3324dc5e59653b0a391155fbae83a216ddd4a13a2793fa23ee611 OP_CHECKSIG OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## A correct script still has an invalid intermediate expression

`t1-human-composed-val-compound-v3-020`

The parent chat answer drops the joint approval's 48-block delay. RL still fails this question. Additional SFT produces a correct final script in both modes, but its stated Miniscript fails type checking in both modes.

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

| Model | Interface | Final script | Policy | Miniscript |
|---|---|---|---|---|
| parent | chat | wrong semantics | equivalent | not equivalent |
| parent | submit | decode reject | equivalent | invalid |
| rl | chat | decode reject | equivalent | invalid |
| rl | submit | decode reject | equivalent | invalid |
| sft | chat | correct | equivalent | invalid |
| sft | submit | correct | equivalent | invalid |

### parent chat answer

Policy:

```text
thresh(2,and(pk(Casey),older(144)),and(and(pk(Cleo),pk(Nora)),older(48)),or(pk(Eli),pk(Wren)))
```

Miniscript:

```text
thresh(2,jc:and_v(v:pk(Cleo),pk_k(Nora)),snj:and_v(v:pk(Casey),older(144)),a:or_b(pk(Eli),s:pk(Wren)))
```

Final answer:

P2WSH witness script:

```text
0314d24c9153a52ec7c4d3f01c534033689b429b65f60c554ac6b83d7cedb43019 OP_CHECKSIG OP_SWAP 03ca39ccc1b88c70669a4154affecb06920910cb379fe2f43a53459a28120387df OP_CHECKSIG OP_BOOLAND OP_SWAP OP_SIZE OP_0NOTEQUAL OP_IF 02f4dd5fec3262cf29eabacff4663e0021624c14c2c44529ced5b707ad696a67e0 OP_CHECKSIGVERIFY 144 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_ADD OP_TOALTSTACK 03bf87c089fcd069d22eb29e971b1b4cc158116320d36794620ac8f63ff9fc2209 OP_CHECKSIG OP_SWAP 0294ba17067549430f009fa692d0b9999b7b9e3ee05d29d9f43dcb580122cf3479 OP_CHECKSIG OP_BOOLOR OP_FROMALTSTACK OP_ADD OP_PUSHNUM_2 OP_EQUAL
```


### rl chat answer

Policy:

```text
thresh(2,and(pk(Casey),older(144)),and(and(pk(Cleo),pk(Nora)),older(48)),or(pk(Eli),pk(Wren)))
```

Miniscript:

```text
thresh(2,and_b(pk(Casey),older(144)),ajc:and_v(v:older(48),and_v(v:pk(Cleo),pk_k(Nora))),a:or_b(pk(Eli),s:pk(Wren)))
```

Final answer:

P2WSH witness script:

```text
02f4dd5fec3262cf29eabacff4663e0021624c14c2c44529ced5b707ad696a67e0 OP_CHECKSIG OP_SWAP 144 OP_CSV OP_BOOLAND OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF 48 OP_CSV OP_VERIFY 0314d24c9153a52ec7c4d3f01c534033689b429b65f60c554ac6b83d7cedb43019 OP_CHECKSIGVERIFY 03ca39ccc1b88c70669a4154affecb06920910cb379fe2f43a53459a28120387df OP_CHECKSIG OP_ENDIF OP_FROMALTSTACK OP_ADD OP_TOALTSTACK 03bf87c089fcd069d22eb29e971b1b4cc158116320d36794620ac8f63ff9fc2209 OP_CHECKSIG OP_SWAP 0294ba17067549430f009fa692d0b9999b7b9e3ee05d29d9f43dcb580122cf3479 OP_CHECKSIG OP_BOOLOR OP_FROMALTSTACK OP_ADD OP_PUSHNUM_2 OP_EQUAL
```


### sft chat answer

Policy:

```text
thresh(2,and(pk(Casey),older(144)),and(and(pk(Cleo),pk(Nora)),older(48)),or(pk(Eli),pk(Wren)))
```

Miniscript:

```text
thresh(2,jc:and_v(v:pk(Casey),older(144)),ajc:and_v(v:older(48),and_v(v:pk(Cleo),pk_k(Nora))),a:or_b(pk(Eli),s:pk(Wren)))
```

Final answer:

P2WSH witness script:

```text
OP_SIZE OP_0NOTEQUAL OP_IF 02f4dd5fec3262cf29eabacff4663e0021624c14c2c44529ced5b707ad696a67e0 OP_CHECKSIGVERIFY 144 OP_CSV OP_ENDIF OP_0NOTEQUAL OP_TOALTSTACK OP_SIZE OP_0NOTEQUAL OP_IF 48 OP_CSV OP_VERIFY 0314d24c9153a52ec7c4d3f01c534033689b429b65f60c554ac6b83d7cedb43019 OP_CHECKSIGVERIFY 03ca39ccc1b88c70669a4154affecb06920910cb379fe2f43a53459a28120387df OP_CHECKSIG OP_ENDIF OP_FROMALTSTACK OP_ADD OP_TOALTSTACK 03bf87c089fcd069d22eb29e971b1b4cc158116320d36794620ac8f63ff9fc2209 OP_CHECKSIG OP_SWAP 0294ba17067549430f009fa692d0b9999b7b9e3ee05d29d9f43dcb580122cf3479 OP_CHECKSIG OP_BOOLOR OP_FROMALTSTACK OP_ADD OP_PUSHNUM_2 OP_EQUAL
```

## RL introduces a missing delay

`t1-human-composed-val-compound-v3-018`

The parent chat answer is correct. The RL chat policy still states the 288-block delay, but the Miniscript and final script drop it. Additional SFT keeps a correct chat answer.

### Request

I am setting up a club reserve. A withdrawal should work with any 2 of these approvals. Please write the Bitcoin script.

- Cleo can give one approval with a signature.
- Nora can give one approval with a signature.
- Wren can give one approval with a signature.
- Vik and Uma share one approval and must both sign for it; their joint approval also needs the output to be at least 288 blocks old, measured from confirmation.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

### Reference policy

```text
thresh(2,pk(Cleo),pk(Nora),pk(Wren),and(and(pk(Vik),pk(Uma)),older(288)))
```

| Model | Interface | Final script | Policy | Miniscript |
|---|---|---|---|---|
| parent | chat | correct | equivalent | equivalent |
| parent | submit | wrong semantics | invalid | invalid |
| rl | chat | wrong semantics | equivalent | not equivalent |
| rl | submit | decode reject | equivalent | invalid |
| sft | chat | correct | equivalent | equivalent |
| sft | submit | correct | equivalent | equivalent |

### parent chat answer

Policy:

```text
thresh(2,pk(Cleo),pk(Nora),pk(Wren),and(and(pk(Vik),pk(Uma)),older(288)))
```

Miniscript:

```text
thresh(2,jc:and_v(v:older(288),and_v(v:pk(Vik),pk_k(Uma))),s:pk(Wren),s:pk(Nora),s:pk(Cleo))
```

Final answer:

P2WSH witness script:

```text
OP_SIZE OP_0NOTEQUAL OP_IF 288 OP_CSV OP_VERIFY 03b3e8c35dd8158678560ec5697746c4ff183b54cedf9180a508b7102d922c31eb OP_CHECKSIGVERIFY 035b78e2915351c715770c99cffb1ae7015bc7c5cf52428fa326f9ec89c33c26e8 OP_CHECKSIG OP_ENDIF OP_SWAP 027d3ebcc469b9fef8e614ac662bbcda375d2a838dc6e3cecd3b558af91dfe977b OP_CHECKSIG OP_ADD OP_SWAP 024270bd13a452b2a66ccd1960b558f1736f8bccd638ed957b550443cf10e31635 OP_CHECKSIG OP_ADD OP_SWAP 033bb211cd7957693cd50e2f8becf4dbf42f1f6eb1ec719cd7bb42c6a52594635a OP_CHECKSIG OP_ADD OP_PUSHNUM_2 OP_EQUAL
```


### rl chat answer

Policy:

```text
thresh(2,pk(Cleo),pk(Nora),pk(Wren),and(and(pk(Vik),pk(Uma)),older(288)))
```

Miniscript:

```text
thresh(2,and_b(pk(Vik),s:pk(Uma)),s:pk(Nora),s:pk(Wren),s:pk(Cleo))
```

Final answer:

P2WSH witness script:

```text
03b3e8c35dd8158678560ec5697746c4ff183b54cedf9180a508b7102d922c31eb OP_CHECKSIG OP_SWAP 035b78e2915351c715770c99cffb1ae7015bc7c5cf52428fa326f9ec89c33c26e8 OP_CHECKSIG OP_BOOLAND OP_SWAP 024270bd13a452b2a66ccd1960b558f1736f8bccd638ed957b550443cf10e31635 OP_CHECKSIG OP_ADD OP_SWAP 027d3ebcc469b9fef8e614ac662bbcda375d2a838dc6e3cecd3b558af91dfe977b OP_CHECKSIG OP_ADD OP_SWAP 033bb211cd7957693cd50e2f8becf4dbf42f1f6eb1ec719cd7bb42c6a52594635a OP_CHECKSIG OP_ADD OP_PUSHNUM_2 OP_EQUAL
```


### sft chat answer

Policy:

```text
thresh(2,pk(Cleo),pk(Nora),pk(Wren),and(and(pk(Vik),pk(Uma)),older(288)))
```

Miniscript:

```text
thresh(2,jc:and_v(v:older(288),and_v(v:pk(Vik),pk_k(Uma))),s:pk(Nora),s:pk(Wren),s:pk(Cleo))
```

Final answer:

P2WSH witness script:

```text
OP_SIZE OP_0NOTEQUAL OP_IF 288 OP_CSV OP_VERIFY 03b3e8c35dd8158678560ec5697746c4ff183b54cedf9180a508b7102d922c31eb OP_CHECKSIGVERIFY 035b78e2915351c715770c99cffb1ae7015bc7c5cf52428fa326f9ec89c33c26e8 OP_CHECKSIG OP_ENDIF OP_SWAP 024270bd13a452b2a66ccd1960b558f1736f8bccd638ed957b550443cf10e31635 OP_CHECKSIG OP_ADD OP_SWAP 027d3ebcc469b9fef8e614ac662bbcda375d2a838dc6e3cecd3b558af91dfe977b OP_CHECKSIG OP_ADD OP_SWAP 033bb211cd7957693cd50e2f8becf4dbf42f1f6eb1ec719cd7bb42c6a52594635a OP_CHECKSIG OP_ADD OP_PUSHNUM_2 OP_EQUAL
```
