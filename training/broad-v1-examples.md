# Broader curriculum: training examples

These eight examples come from training. Reserved questions are not shown here. Requests below omit their public-key appendix; the actual model prompt includes it. Public keys in reference expressions are replaced with request labels for review. These labelled expressions are explanatory, not runnable scripts.

## guarded-choice (write)

`t1-human-composed-train-broad-v1-guarded-choice-000-write`

Can you write the Bitcoin script for this reserve?

Dara signs for either group's route. In addition, either at least 2 of Luz and Gia sign, or at least 2 of Kai, Inez, and Hugo sign. Whichever group signs, the output meets a relative time lock of 2048 seconds, using 4 units of 512 seconds on Bitcoin's relative-lock clock. The guard cannot replace either group.

Allow every spend that meets these rules and no other route.

Reference policy:

```text
and(pk(Dara),and(or(and(pk(Luz),pk(Gia)),thresh(2,pk(Kai),pk(Inez),pk(Hugo))),older(4194308)))
```

Reference Miniscript:

```text
and_v(v:pk(Dara),and_v(andor(pk(Luz),v:pk(Gia),v:multi(2,Kai,Inez,Hugo)),older(4194308)))
```

## team-recovery (tree)

`t4-human-composed-train-broad-v1-team-recovery-000-tree`

Please give me a Taproot tr() descriptor for our wallet. Dara signs to spend immediately without any other approval, secret, or waiting period. The rules in the next paragraph apply only to the alternative spending arrangement, not to Dara's route:

This arrangement allows two spending routes. The first works when at least 3 of Hugo, Luz, and Finn sign, provided that the 32-byte secret matching HASH160 3bf68f1b354fadc52367eaa7ab12772ffd4447e8 is supplied. The second works when at least 3 of Eli, Ari, and Bea sign, provided that the 32-byte secret matching HASH160 3bf68f1b354fadc52367eaa7ab12772ffd4447e8 is supplied. Either complete route is enough.

The alternative arrangement does not require Dara to sign. Allow all of these routes and no others. Choose the internal key and leaves to reduce the worst-case input weight.

Reference policy:

```text
or(pk(Dara),or(and(and(pk(Hugo),and(pk(Luz),pk(Finn))),hash160(3bf68f1b354fadc52367eaa7ab12772ffd4447e8)),and(and(pk(Eli),and(pk(Ari),pk(Bea))),hash160(3bf68f1b354fadc52367eaa7ab12772ffd4447e8))))
```

Reference descriptor:

```text
tr(Dara,{and_v(v:and_v(v:pk(Hugo),and_v(v:pk(Luz),pk(Finn))),hash160(3bf68f1b354fadc52367eaa7ab12772ffd4447e8)),and_v(v:and_v(v:pk(Eli),and_v(v:pk(Ari),pk(Bea))),hash160(3bf68f1b354fadc52367eaa7ab12772ffd4447e8))})
```

## secret-delivery (write)

`t1-human-composed-train-broad-v1-secret-delivery-000-write`

I need a Bitcoin script with the following spending rules.

Delivery can be authorized when at least 2 of Eli and Kai sign and the 32-byte secret matching SHA-256 8596a55759bb2aa17943536ce8fd6730806762bcd7922ba55a91bb47888a6473 is supplied. A separate recovery route works when at least 3 of Dara, Bea, and Finn sign, but only when both of these clock requirements hold: the output is at least 24 blocks old, measured from confirmation, and the spending transaction has time-based locktime (Unix seconds) of at least 1806145591. Recovery does not need the secret.

Allow every spend that meets these rules and no other route.

Reference policy:

```text
or(and(and(pk(Eli),pk(Kai)),sha256(8596a55759bb2aa17943536ce8fd6730806762bcd7922ba55a91bb47888a6473)),and(and(pk(Dara),and(pk(Bea),pk(Finn))),and(older(24),after(1806145591))))
```

Reference Miniscript:

```text
or_i(and_v(v:and_v(v:pk(Eli),pk(Kai)),sha256(8596a55759bb2aa17943536ce8fd6730806762bcd7922ba55a91bb47888a6473)),and_v(v:and_v(v:pk(Dara),and_v(v:pk(Bea),pk(Finn))),and_v(v:older(24),after(1806145591))))
```

## joint-departments (write)

`t1-human-composed-train-broad-v1-joint-departments-000-write`

Please help me make a Bitcoin script for our shared funds.

Both departments must approve: at least 3 of Dara, Bea, Kai, and Cleo sign, and at least 2 of Inez, Finn, and Hugo sign. Extra signatures from one department cannot replace approval from the other. We also require that the 32-byte secret matching SHA-256 4d016746860d774eb22971625528dcc4b67f965b0f673934fd6658fef87c2e88 is supplied.

Allow every spend that meets these rules and no other route.

Reference policy:

```text
and(thresh(3,pk(Dara),pk(Bea),pk(Kai),pk(Cleo)),and(thresh(2,pk(Inez),pk(Finn),pk(Hugo)),sha256(4d016746860d774eb22971625528dcc4b67f965b0f673934fd6658fef87c2e88)))
```

Reference Miniscript:

```text
and_v(v:multi(3,Dara,Bea,Kai,Cleo),and_v(v:multi(2,Inez,Finn,Hugo),sha256(4d016746860d774eb22971625528dcc4b67f965b0f673934fd6658fef87c2e88)))
```

## three-routes (write)

`t1-human-composed-train-broad-v1-three-routes-000-write`

Can you write the Bitcoin script for this reserve?

Within this arrangement, any one of three routes is enough: at least 2 of Gia and Hugo sign with no waiting or secret; at least 3 of Luz, Dara, and Jules sign together with the requirement that the output meets a relative time lock of 73728 seconds, using 144 units of 512 seconds on Bitcoin's relative-lock clock; or Bea signs together with the requirement that the spending transaction has block-height locktime of at least 986538. Conditions belong only to their stated route.

Allow every spend that meets these rules and no other route.

Reference policy:

```text
or(and(pk(Gia),pk(Hugo)),or(and(and(pk(Luz),and(pk(Dara),pk(Jules))),older(4194448)),and(pk(Bea),after(986538))))
```

Reference Miniscript:

```text
or_i(and_v(v:pk(Gia),pk(Hugo)),or_i(and_v(v:and_v(v:pk(Luz),and_v(v:pk(Dara),pk(Jules))),older(4194448)),and_v(v:pk(Bea),after(986538))))
```

## staged-quorums (tree)

`t4-human-composed-train-broad-v1-staged-quorums-000-tree`

Please give me a Taproot tr() descriptor for our wallet. Hugo signs to spend immediately without any other approval, secret, or waiting period. The rules in the next paragraph apply only to the alternative spending arrangement, not to Hugo's route:

The normal route is available whenever at least 2 of Inez and Eli sign. A backup route works when at least 2 of Ari, Jules, and Bea sign, provided that the output meets a relative time lock of 147456 seconds, using 288 units of 512 seconds on Bitcoin's relative-lock clock. A third route works when at least 2 of Gia and Dara sign, provided both that the output meets a relative time lock of 6144 seconds, using 12 units of 512 seconds on Bitcoin's relative-lock clock and that the 32-byte secret matching SHA-256 e018db8e8b26a40cac43a2fb3a603fa13cadc5c011edac78c272045b562ff00e is supplied. Each route stays available once its conditions hold.

The alternative arrangement does not require Hugo to sign. Allow all of these routes and no others. Choose the internal key and leaves to reduce the worst-case input weight.

Reference policy:

```text
or(pk(Hugo),or(and(pk(Inez),pk(Eli)),or(and(thresh(2,pk(Ari),pk(Jules),pk(Bea)),older(4194592)),and(and(pk(Gia),pk(Dara)),and(older(4194316),sha256(e018db8e8b26a40cac43a2fb3a603fa13cadc5c011edac78c272045b562ff00e))))))
```

Reference descriptor:

```text
tr(Hugo,{{and_v(v:pk(Inez),pk(Eli)),and_v(v:multi_a(2,Ari,Jules,Bea),older(4194592))},and_v(v:and_v(v:pk(Gia),pk(Dara)),and_v(v:sha256(e018db8e8b26a40cac43a2fb3a603fa13cadc5c011edac78c272045b562ff00e),older(4194316)))})
```

## two-clock-gate (write)

`t1-human-composed-train-broad-v1-two-clock-gate-000-write`

Can you write the Bitcoin script for this reserve?

To use this arrangement, at least 2 of Kai, Cleo, and Hugo sign. Signing is not enough on its own: the output meets a relative time lock of 147456 seconds, using 288 units of 512 seconds on Bitcoin's relative-lock clock, and the spending transaction has time-based locktime (Unix seconds) of at least 1882274667. Both clocks must pass; either one alone is insufficient.

Allow every spend that meets these rules and no other route.

Reference policy:

```text
and(thresh(2,pk(Kai),pk(Cleo),pk(Hugo)),and(older(4194592),after(1882274667)))
```

Reference Miniscript:

```text
and_v(v:multi(2,Kai,Cleo,Hugo),and_v(v:older(4194592),after(1882274667)))
```

## global-secret (tree)

`t4-human-composed-train-broad-v1-global-secret-000-tree`

Please give me a Taproot tr() descriptor for our wallet. Luz signs to spend immediately without any other approval, secret, or waiting period. The rules in the next paragraph apply only to the alternative spending arrangement, not to Luz's route:

Both of the following routes require that the 32-byte secret matching HASH160 4a160dafc61824e5c048e5da0fad021eb26fb2bb is supplied. With that secret, one route works when at least 3 of Gia, Ari, and Inez sign. The other works when at least 2 of Cleo, Dara, and Kai sign, but only if the output meets a relative time lock of 73728 seconds, using 144 units of 512 seconds on Bitcoin's relative-lock clock. The second route still needs the secret.

The alternative arrangement does not require Luz to sign. Allow all of these routes and no others. Choose the internal key and leaves to reduce the worst-case input weight.

Reference policy:

```text
or(pk(Luz),and(hash160(4a160dafc61824e5c048e5da0fad021eb26fb2bb),or(and(pk(Gia),and(pk(Ari),pk(Inez))),and(thresh(2,pk(Cleo),pk(Dara),pk(Kai)),older(4194448)))))
```

Reference descriptor:

```text
tr(Luz,and_v(v:hash160(4a160dafc61824e5c048e5da0fad021eb26fb2bb),andor(pk(Gia),and_v(v:pk(Ari),pk(Inez)),and_v(v:multi_a(2,Cleo,Dara,Kai),older(4194448)))))
```
