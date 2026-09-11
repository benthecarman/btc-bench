Fixed cases chosen before the longer run completed. These are development examples, not a random estimate of accuracy.

**wp-native-single-0-template**

I need a native SegWit P2WPKH wallet controlled by Alice alone. Allow exactly these spending routes.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Alice: @0


parent, chat. Grade: `{"task_id": "wp-native-single-0-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "UnrecognizedFragment"}`

```text


Descriptor template:

```text
388(pk(@0/**))
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: pk(@0)
P2WPKH is the P2WSH witness type, so the registry key is the script hash of the P2WPKH script.
The registry key for the policy is: OP_CHECKSIG @0/**.
The BIP-388 descriptor template: 388(pk(@0/**))

```

parent, submit. Grade: `{"task_id": "wp-native-single-0-template", "score": 1.0, "failure": null, "reason": null}`

```text
wpkh(@0/**)
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: pk(@0)
P2WPKH is the only wrapper needed here.

```

rl-step256, chat. Grade: `{"task_id": "wp-native-single-0-template", "score": 1.0, "failure": null, "reason": null}`

```text


Descriptor template:

```text
wpkh(@0/**)
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: pk(@0)
P2WPKH descriptor body:
@0/** and the key list managed separately

```

rl-step256, submit. Grade: `{"task_id": "wp-native-single-0-template", "score": 1.0, "failure": null, "reason": null}`

```text
wpkh(@0/**)
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: pk(@0)
P2WPKH descriptor base: wpkh(@0/**)

```

mixed-step256, chat. Grade: `{"task_id": "wp-native-single-0-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "TrailingInput"}`

```text


Descriptor template:

```text
wpkh(@0/**)#P2WPKH
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: pk(@0)
P2WPKH is the P2WSH output type, so the wallet descriptor ends in #P2WPKH.
In the absence of any key list management policy, the derived keys fall back to the original keyset.

```

mixed-step256, submit. Grade: `{"task_id": "wp-native-single-0-template", "score": 1.0, "failure": null, "reason": null}`

```text
wpkh(@0/**)
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: pk(@0)
P2WPKH is the only wrapper needed here.

```

**wp-joint-councils-0-template**

Set up a Taproot reserve. Alice has an immediate owner route. The alternative route needs both council approvals: at least two of Bob, Cleo, and Dara, and at least one of Eli and Finn. Extra signatures in one council cannot replace the other council approval. The councils do not need Alice to sign. Allow exactly these spending routes.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Alice: @0
Bob: @1
Cleo: @2
Dara: @3
Eli: @4
Finn: @5


parent, chat. Grade: `{"task_id": "wp-joint-councils-0-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "the script is valid but not semantically equivalent to the required spending policy"}`

```text


Descriptor template:

```text
tr(@0/**,and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),pk(@5/**))))
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),pk(@5/**)))
The Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.

```

parent, submit. Grade: `{"task_id": "wp-joint-councils-0-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "the script is valid but not semantically equivalent to the required spending policy"}`

```text
tr(@0/**,and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),pk(@5/**))))
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),pk(@5/**)))
The Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.

```

rl-step256, chat. Grade: `{"task_id": "wp-joint-councils-0-template", "score": 1.0, "failure": null, "reason": null}`

```text


Descriptor template:

```text
tr(@0/**,and_v(v:multi_a(2,@1/**,@2/**,@3/**),multi_a(1,@4/**,@5/**)))
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(thresh(2,pk(@1),pk(@2),pk(@3)),thresh(1,pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:multi_a(2,@1/**,@2/**,@3/**),multi_a(1,@4/**,@5/**))
The Taproot internal key supplies the immediate owner route; the alternatives supply the remaining routes.

```

rl-step256, submit. Grade: `{"task_id": "wp-joint-councils-0-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "InvalidSyntax"}`

```text
tr(@0/**,{and_v(v:multi_a(2,@1/**,@2/**,@3/**)),multi_a(1,@4/**,@5/**)})}
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(thresh(2,pk(@1),pk(@2),pk(@3)),thresh(1,pk(@4),pk(@5))))
Taproot leaf Miniscript:
pk(@0/**)
and_v(v:multi_a(2,@1/**,@2/**,@3/**),multi_a(1,@4/**,@5/**))
The Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.

```

mixed-step256, chat. Grade: `{"task_id": "wp-joint-councils-0-template", "score": 1.0, "failure": null, "reason": null}`

```text


Descriptor template:

```text
tr(@0/**,and_v(v:multi_a(2,@1/**,@2/**,@3/**),multi_a(1,@4/**,@5/**)))
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(thresh(2,pk(@1),pk(@2),pk(@3)),thresh(1,pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:multi_a(2,@1/**,@2/**,@3/**),multi_a(1,@4/**,@5/**))
The Taproot internal key supplies the immediate owner route; the alternatives supply the remaining routes.

```

mixed-step256, submit. Grade: `{"task_id": "wp-joint-councils-0-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "InvalidSyntax"}`

```text
tr(@0/**,{and_v(v:multi_a(2,@1/**,@2/**,@3/**),multi_a(1,@4/**,@5/**))})
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(thresh(2,pk(@1),pk(@2),pk(@3)),thresh(1,pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:multi_a(2,@1/**,@2/**,@3/**),multi_a(1,@4/**,@5/**))
The Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.

```

**wr1-age-and-height-template**

For this Taproot wallet, Main has unrestricted access. Recovery alone may also spend, but must meet BOTH requirements: the coins are at least 400 blocks old from confirmation and the spending transaction has a block-height locktime of at least 934567. Meeting just one requirement is insufficient. No other route.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Main: @0
Recovery: @1


parent, chat. Grade: `{"task_id": "wr1-age-and-height-template", "score": 1.0, "failure": null, "reason": null}`

```text


Descriptor template:

```text
tr(@0/**,and_v(v:pk(@1/**),and_v(v:older(400),after(934567))))
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(pk(@1),older(400),after(934567)))
Taproot leaf Miniscript:
and_v(v:pk(@1),and_v(v:older(400),after(934567)))
Taproot internal key placeholder:
@1/** and_v(v:older(400),after(934567))

```

parent, submit. Grade: `{"task_id": "wr1-age-and-height-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "TrailingInput"}`

```text
tr(@0/**,and_v(v:pk(@1/**),and_v(v:older(400),after(934567)))))
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(pk(@1),and(older(400),after(934567))))
Taproot leaf Miniscript:
and_v(v:pk(@1),and_v(v:older(400),after(934567)))
The Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.

```

rl-step256, chat. Grade: `{"task_id": "wr1-age-and-height-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "InvalidSyntax"}`

```text


Descriptor template:

```text
tr(@0/**,{and_v(v:pk(@1/**),and_v(v:older(400),after(934567)))})
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(pk(@1),older(400),after(934567)))
Taproot leaf Miniscript:
and_v(v:pk(@1),and_v(v:older(400),after(934567)))
Taproot internal key placeholder:
@0/** and @1/** and older(400) and after(934567)

```

rl-step256, submit. Grade: `{"task_id": "wr1-age-and-height-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "InvalidSyntax"}`

```text
tr(@0/**,{pk(@1/**),older(400),after(934567)})
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(pk(@1),older(400),after(934567)))
Taproot leaf Miniscript:
pk(@1/**)
The Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.

```

mixed-step256, chat. Grade: `{"task_id": "wr1-age-and-height-template", "score": 1.0, "failure": null, "reason": null}`

```text


Descriptor template:

```text
tr(@0/**,and_v(v:pk(@1/**),and_v(v:older(400),after(934567))))
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(pk(@1),older(400),after(934567)))
Taproot leaf Miniscript:
and_v(v:pk(@1),and_v(v:older(400),after(934567)))
Taproot internal key placeholder:
@1/**(and_v(v:older(400),after(934567)))

```

mixed-step256, submit. Grade: `{"task_id": "wr1-age-and-height-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "InvalidSyntax"}`

```text
tr(@0/**,{and_v(v:pk(@1/**),and_v(v:older(400),after(934567)))})
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(pk(@1),older(400),after(934567)))
Taproot leaf Miniscript:
and_v(v:pk(@1),and_v(v:older(400),after(934567)))
The Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.

```

**wr1-quorum-or-joint-template**

The Taproot wallet should allow Owner to spend alone. Two other independent options are allowed: any two of A, B and C, or D and E together. All options work without delay, and each must satisfy its own signature rule. Add no other access.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Owner: @0
A: @1
B: @2
C: @3
D: @4
E: @5


parent, chat. Grade: `{"task_id": "wr1-quorum-or-joint-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "the script is valid but not semantically equivalent to the required spending policy"}`

```text


Descriptor template:

```text
tr(@0/**,and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),pk(@5/**))))
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),or(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),pk(@5/**)))
The Taproot internal key supplies the unrestricted owner key; each leaf supplies an alternative restriction.

```

parent, submit. Grade: `{"task_id": "wr1-quorum-or-joint-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "InvalidSyntax"}`

```text
tr(@0/**,and_v(v:pk(@4),pk(@5)),multi_a(2,@1/**,@2/**,@3/**))
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),or(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:pk(@4),pk(@5))
multi_a(2,@1/**,@2/**,@3/**)
The Taproot internal key supplies the unrestricted owner access; each leaf supplies an alternative path.

```

rl-step256, chat. Grade: `{"task_id": "wr1-quorum-or-joint-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "the script is valid but not semantically equivalent to the required spending policy"}`

```text


Descriptor template:

```text
tr(@0/**,and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),pk(@5/**))))
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),or(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),pk(@5/**)))
The Taproot internal key supplies the unrestricted owner key; each leaf supplies an alternative key set.

```

rl-step256, submit. Grade: `{"task_id": "wr1-quorum-or-joint-template", "score": 0.0, "failure": "missing or ambiguous answer", "reason": "missing final answer"}`

```text
(missing final answer)
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),or(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:pk(@4),pk(@5))
multi_a(2,@1/**,@2/**,@3/**)
The Taproot internal key supplies the unrestricted owner access; each leaf supplies an alternative path.

```

mixed-step256, chat. Grade: `{"task_id": "wr1-quorum-or-joint-template", "score": 1.0, "failure": null, "reason": null}`

```text


Descriptor template:

```text
tr(@0/**,{and_v(v:pk(@4/**),pk(@5/**)),multi_a(2,@1/**,@2/**,@3/**)})
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),or(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:pk(@4),pk(@5))
multi_a(2,@1/**,@2/**,@3/**)
The Taproot internal key supplies the unrestricted owner key; each leaf supplies an alternative key set.

```

mixed-step256, submit. Grade: `{"task_id": "wr1-quorum-or-joint-template", "score": 0.0, "failure": "invalid or wrong contract", "reason": "InvalidSyntax"}`

```text
tr(@0/**,{and_v(v:pk(@4),pk(@5)),multi_a(2,@1/**,@2/**,@3/**)})
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),or(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:pk(@4),pk(@5))
multi_a(2,@1/**,@2/**,@3/**)
The Taproot internal key supplies the unrestricted owner access; each leaf supplies an alternative path.

```

**t4-human-tree-recovery**

Can you put together a tr() descriptor for my wallet? My owner key can spend immediately, or my backup key can spend once the output is 1008 blocks old. Both remain valid after that. Those are the only spending options. Please arrange it to keep the most expensive spend as small as possible.

owner: 70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2
backup: 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2


parent, chat. Grade: `{"task_id": "t4-human-tree-recovery", "score": 0.0, "size_score": null, "reason": "not a valid descriptor: unrecognized name '70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF'", "failure": "parse error"}`

```text


Bitcoin script:

```text
70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Let me work through the spending conditions.
As a policy that is: or(pk(70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2),and(pk(615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2),older(1008)))
In Miniscript: or_b(pk(70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2),and_v(v:pk(615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2),older(1008)))
Which encodes to:
70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF


Bitcoin script:

```text
70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF
```
```

parent, submit. Grade: `{"task_id": "t4-human-tree-recovery", "score": 0.0, "size_score": null, "reason": "not a valid descriptor: unrecognized name '615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2'", "failure": "parse error"}`

```text
{"task": "descriptor", "descriptor": "tr(70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2,{615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2,older(1008)})"}
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use the supplied owner key as the internal key, and arrange the external key material as a relative timelock.
That gives the descriptor:
tr(70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2,{615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2,older(1008)})



```

rl-step256, chat. Grade: `{"task_id": "t4-human-tree-recovery", "score": 0.0, "size_score": null, "reason": "not a valid descriptor: unrecognized name '70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF'", "failure": "parse error"}`

```text


Bitcoin script:

```text
70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Let me work through the spending conditions.
As a policy that is: or(pk(70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2),and(pk(615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2),older(1008)))
In Miniscript: or_d(pk(70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2),and_v(v:pk(615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2),older(1008)))
Which encodes to:
70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF


Bitcoin script:

```text
70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF
```
```

rl-step256, submit. Grade: `{"task_id": "t4-human-tree-recovery", "score": 0.0, "size_score": null, "reason": "not a valid descriptor: unknown wrapper \u00ab2:\u00bb", "failure": "parse error"}`

```text
{"task": "descriptor", "descriptor": "tr(70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2,615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2:older(1008))"}
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use the supplied owner key as the internal key in a Taproot output, and arrange the backup key as a use-after-older-than witness.



```

mixed-step256, chat. Grade: `{"task_id": "t4-human-tree-recovery", "score": 0.0, "size_score": null, "reason": "not a valid descriptor: unrecognized name '70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF'", "failure": "parse error"}`

```text


Bitcoin script:

```text
70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF
```
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Let me work through the spending conditions.
As a policy that is: or(pk(70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2),and(pk(615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2),older(1008)))
In Miniscript: or_d(pk(70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2),and_v(v:pk(615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2),older(1008)))
Which encodes to:
70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF


Bitcoin script:

```text
70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2 OP_CHECKSIG OP_IFDUP OP_NOTIF 615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2 OP_CHECKSIGVERIFY 1008 OP_CSV OP_ENDIF
```
```

mixed-step256, submit. Grade: `{"task_id": "t4-human-tree-recovery", "score": 0.0, "size_score": null, "reason": "not a valid descriptor: unknown wrapper \u00ab2:\u00bb", "failure": "parse error"}`

```text
{"task": "descriptor", "descriptor": "tr(70750574e7c4f8c5e93e177340cbd71d1c1bd87464c28ca987e80c966caffee2,615db8363840dce41619fb41453bd61a4e3ddaa5a637372bd6ed8e7d1304ccd2:older(1008))"}
```

Trace excerpt (first 2,000 characters; full response is saved in the run directory):

```text

Use the supplied owner key as the internal key in a Taproot output, and arrange the backup key as a use-after-older-than witness.



```
