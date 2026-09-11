# Expanded compound-approval examples

Requests are synthetic. Display names stand for the full public keys, and H256/H160 stand for the supplied digests. References use P2WSH witness scripts. The compiled fixtures contain the concrete keys and hashes.

## train 0: deadline, pair, either

For our club reserve, the spending rule is at least 2 approvals out of these 3.

1. Casey gives one approval with a signature, but it needs the spending transaction to have block-height locktime at least 978514.
2. Ada and Tess share one approval and must both sign for it.
3. Nora and Wren share one approval, which either person can give alone; both signing still counts once.

Count each listed approval once, even if it has several signatures or a secret. Extra approvals are fine. Do not add another way to spend. Write the Bitcoin script.

Policy:

```text
thresh(2,and(pk(Casey),after(978514)),and(pk(Ada),pk(Tess)),or(pk(Nora),pk(Wren)))
```

Verified Miniscript:

```text
thresh(2,and_b(pk(Ada),s:pk(Tess)),snj:and_v(v:pk(Casey),after(978514)),a:or_b(pk(Nora),s:pk(Wren)))
```

## train 1: pair, sha, committee

For our charity fund, the spending rule is at least 2 approvals out of these 3.

1. Owen and Casey share one approval and must both sign for it.
2. Alex gives one approval with a signature and the 32-byte secret matching SHA-256 H256; neither item alone is enough.
3. Nora, Wren and Ben form a committee; any two must sign to give its one approval.

Count each listed approval once, even if it has several signatures or a secret. Extra approvals are fine. Do not add another way to spend. Write the Bitcoin script.

Policy:

```text
thresh(2,and(pk(Owen),pk(Casey)),and(pk(Alex),sha256(H256)),thresh(2,pk(Nora),pk(Wren),pk(Ben)))
```

Verified Miniscript:

```text
thresh(2,and_b(pk(Owen),s:pk(Casey)),aj:and_v(v:pk(Alex),sha256(H256)),a:multi(2,Nora,Wren,Ben))
```

## train 2: either, delay, hash160

For our charity fund, the spending rule is at least 2 approvals out of these 3.

1. Mina and Blair share one approval, which either person can give alone; both signing still counts once.
2. Owen gives one approval with a signature, but it can count only with a relative time lock of at least 2048 seconds, using 4 units of 512 seconds on Bitcoin's relative-lock clock.
3. Ada gives one approval with a signature and the 32-byte secret matching HASH160 H160; neither item alone is enough.

Count each listed approval once, even if it has several signatures or a secret. Extra approvals are fine. Do not add another way to spend. Write the Bitcoin script.

Policy:

```text
thresh(2,or(pk(Mina),pk(Blair)),and(pk(Owen),older(4194308)),and(pk(Ada),hash160(H160)))
```

Verified Miniscript:

```text
thresh(2,or_b(pk(Mina),s:pk(Blair)),snj:and_v(v:pk(Owen),older(4194308)),aj:and_v(v:pk(Ada),hash160(H160)))
```

## train 3: hash160, pair, pair, hash160

Please set up this charity fund so that any 2 approvals from this list can spend.

1. Blair gives one approval with a signature and the 32-byte secret matching HASH160 H160; neither item alone is enough.
2. Vik and Alex share one approval and must both sign for it.
3. Nora and Wren share one approval and must both sign for it.
4. Ben gives one approval with a signature and the 32-byte secret matching HASH160 H160; neither item alone is enough.

Count each listed approval once, even if it has several signatures or a secret. Extra approvals are fine. Do not add another way to spend. Write the Bitcoin script.

Policy:

```text
thresh(2,and(pk(Blair),hash160(H160)),and(pk(Vik),pk(Alex)),and(pk(Nora),pk(Wren)),and(pk(Ben),hash160(H160)))
```

Verified Miniscript:

```text
thresh(2,j:and_v(v:pk(Blair),hash160(H160)),a:and_b(pk(Vik),s:pk(Alex)),a:and_b(pk(Nora),s:pk(Wren)),aj:and_v(v:pk(Ben),hash160(H160)))
```

## validation 0: either, deadline, sha, pair

I am setting up a co-op wallet. A withdrawal should work with any 2 of these approvals. Please write the Bitcoin script.

- Nora and Owen share one approval, which either person can give alone; both signing still counts once.
- Uma gives one approval with a signature, but it needs the spending transaction to have block-height locktime at least 959035.
- Ada gives one approval with a signature and the 32-byte secret matching SHA-256 H256; neither item alone is enough.
- Pia and Ben share one approval and must both sign for it.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

Policy:

```text
thresh(2,or(pk(Nora),pk(Owen)),and(pk(Uma),after(959035)),and(pk(Ada),sha256(H256)),and(pk(Pia),pk(Ben)))
```

Verified Miniscript:

```text
thresh(2,or_b(pk(Nora),s:pk(Owen)),snj:and_v(v:pk(Uma),after(959035)),aj:and_v(v:pk(Ada),sha256(H256)),a:and_b(pk(Pia),s:pk(Ben)))
```

## validation 24: person, delayed_committee, deadline

I am setting up a community fund. A withdrawal should work with any 2 of these approvals. Please write the Bitcoin script.

- Tess can give one approval with a signature.
- Pia, Sam and Uma form a committee; any two must sign to give its one approval, and that approval also needs the output to be at least 288 blocks old, measured from confirmation.
- Casey gives one approval with a signature, but it needs the spending transaction to have block-height locktime at least 954808.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

Policy:

```text
thresh(2,pk(Tess),and(thresh(2,pk(Pia),pk(Sam),pk(Uma)),older(288)),and(pk(Casey),after(954808)))
```

Verified Miniscript:

```text
thresh(2,nj:and_v(v:multi(2,Pia,Sam,Uma),older(288)),s:pk(Tess),snj:and_v(v:pk(Casey),after(954808)))
```

## validation 25: person, either, secret_pair

I am setting up a community fund. A withdrawal should work with any 2 of these approvals. Please write the Bitcoin script.

- Casey can give one approval with a signature.
- Ben and Ada share one approval, which either person can give alone; both signing still counts once.
- Tess and Mina share one approval and must both sign for it; their joint approval also needs the 32-byte secret matching SHA-256 H256.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

Policy:

```text
thresh(2,pk(Casey),or(pk(Ben),pk(Ada)),and(and(pk(Tess),pk(Mina)),sha256(H256)))
```

Verified Miniscript:

```text
thresh(2,or_b(pk(Ben),s:pk(Ada)),s:pk(Casey),ajc:and_v(v:sha256(H256),and_v(v:pk(Tess),pk_k(Mina))))
```

## validation 26: delayed_pair, hash160, either

Can you write the Bitcoin script for our community fund? We need at least 2 approvals to release the money.

- Mina and Tess share one approval and must both sign for it; their joint approval also needs a relative time lock of at least 6144 seconds, using 12 units of 512 seconds on Bitcoin's relative-lock clock.
- Pia gives one approval with a signature and the 32-byte secret matching HASH160 H160; neither item alone is enough.
- Eli and Alex share one approval, which either person can give alone; both signing still counts once.

Each item above counts as one approval. Extra signatures within an item do not add approvals. Allow every combination that meets these rules and no other spending route.

Policy:

```text
thresh(2,and(and(pk(Mina),pk(Tess)),older(4194316)),and(pk(Pia),hash160(H160)),or(pk(Eli),pk(Alex)))
```

Verified Miniscript:

```text
thresh(2,jc:and_v(v:older(4194316),and_v(v:pk(Mina),pk_k(Tess))),aj:and_v(v:pk(Pia),hash160(H160)),a:or_b(pk(Eli),s:pk(Alex)))
```
