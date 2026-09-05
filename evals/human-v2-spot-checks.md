# Human-v2 requests for joint review

These are six exact generated prompts, including test keys and digests.
Check whether each request says what the expected behavior below requires.
The full set is in `datasets/human-v2/prompts.md`.

## Separate committees

`t1-human-two-committee-quorums-both`

```text
Two committees share this wallet. A payment needs any two North signatures AND any two South signatures. Even all three North members cannot act without South. Give me the tapscript leaf; there are no other paths.

north1: c0f4518bbc33c02bc2e3fe57e79590467ebafa305a4cf92af7c5b6ebdee5a89a
north2: 4c83122ee6e8ba5b0975f345deb6e6251456be014d0da5df435b1445b9fac0ec
north3: bbf78d70e134463ad8085bdbbd605243abead0e9d0666e962db94a7bfe2c6edc
south1: 3ba4ba4e829a1bf931accb1ee4dfb8272bd2c50e39cde1996106c94186c0e7d4
south2: 536a561502d696dce28db5619912bbf46f72b42c43b4e6efb7134d3e04dfac8d
south3: 2687d92f919945c3317a19eb43082a7bbe2c4c9a3e41ac6580ca258b127ffae8
```

Expected: Two North and two South signatures work; all three North signatures alone fail. Its paired case lets either committee act alone.

## Owner remains required

`t1-human-owner-scope-owner-stays`

```text
The owner key must always sign. Either helper can countersign now; after 2016 blocks the recovery key may countersign too. Recovery must never bypass the owner. Can you write the Bitcoin script?

owner: 022563959b93faf848141e74ddc33029a5899551a8dbfef9c96ae6b5183dee67ec
helper1: 02f5bc2361ccf380c606bebafedd3bd79f18d8bdd02aff3177627da090062e2c98
helper2: 0396ebf2af197aecc16273341df4ea0a7a36d22a4c4ce982fb2c84990f269dc918
recovery: 035b93b7b12513ef46bfb70d3880c8fb47f98753f1591ae466881e3144305de61c
```

Expected: Recovery alone fails even after 2016 blocks. Owner plus recovery starts working at that age. Its paired case allows independent recovery.

## Three-stage recovery

`t1-human-three-stage-quorum-single-late`

```text
Can you write a tapscript leaf for a three-stage wallet? All three keys work immediately. Any two also work after 144 blocks from confirmation. After 1008 blocks, any one key works. Earlier options remain valid; a signature is always needed.

A: 3f942510e79c3e9f3461924ab94777917a6d20dc6ee67b29e90e7e2b82f5744b
B: 43e1fc09191e81bb15e8cd65121375cbcc1d9879836b5b50092da6269c6913b5
C: e6c87c2c3a2957bc875c2914bac1715469319464a732a98c73dc337dd6d2d87b
```

Expected: Three signatures work now, two start at age 144 blocks, and one starts at age 1008 blocks. No unsigned spend works.

## Secret on every route

`t1-human-shared-secret-scope-all-routes`

```text
A and B together may spend now, or recovery may sign after 576 blocks from confirmation. Either route must also reveal the 32-byte secret for SHA-256 309ebe41bcf413934fc015864ba9677555c0cf62e14d8f7f04783eb1f67dded0. Recovery cannot skip it. What Bitcoin script implements exactly these options?

A: 038a487d5ca545c9478992e9295cc20737263c0eb10d3dda830a8a2c364942acb7
B: 0371e53404778859de572c7c4e3c4e1e2db904d052975cccf374888901d595043a
recovery: 0309b5354b342d1fd930669b6227aac829a8b90d0d191e94e1711af5dd27b81744
```

Expected: The secret is required even for delayed recovery. Its paired case removes the secret from recovery only.

## Seconds versus blocks

`t1-human-relative-lock-domain-seconds`

```text
Please write a Bitcoin script requiring my signature and a relative time lock of 1024 seconds, measured by the Bitcoin relative-lock clock from confirmation. This means two 512-second units, not 1024 blocks. No alternate route.

owner: 027c6d2d5851c7cd51c08e0115171bed6f6a5981e4916c426f6cfff163f0758012
```

Expected: The relative lock uses the time flag and a count of two 512-second units. Its paired case requires 1024 blocks.

## Recovery quorum and secret

`t4-human-tree-large-recovery-secret-two`

```text
Owner can always spend. Recovery needs any two of A/B/C/D/E, age 1008 blocks, and the 32-byte preimage of SHA-256 1bb520ca592f196ba01c7efec84da40193b77ddce2aa8a6e3d2a0eae6d0a17de. No recovery spend may skip the secret. Give me a tr() descriptor with only those routes and the smallest worst-case input weight you can achieve.

owner: bf3f05512bdfa801d017c4999a12d09d62eefdc0a38466cfe9e1e551956acbba
A: e8fccbfe5c9d29f2a1dca52c06d137db152f7296fdb8b3d2ffe528c2a4ce07ec
B: cb5fe3086dfc4ef70975937a5433911b17eeef43db54b7470eb5c1a6d262dceb
C: 5bb885853ec868bed73959424b5da594ecc7b53e95b3f3f1155e921ae284984a
D: 62ec34ee195440e5c3103e13a72e74eb7ee35ed006445fda11e24dc0d07deb59
E: b1aa99da10543a3fadf8ef9cdc5dbfcf3963776123fa6e0c93e05154a99ca649
```

Expected: Owner access is immediate. Recovery needs two of five signatures, age 1008 blocks and the secret together. Grade correctness separately from weight.
