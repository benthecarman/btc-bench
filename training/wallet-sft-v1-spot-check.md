**wp-native-single-0-template**

I need a native SegWit P2WPKH wallet controlled by Alice alone. Allow exactly these spending routes.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Alice: @0


Reference policy: `pk(@0)`

chat: score 0.0; finish stop; reason UnrecognizedFragment

```text


Descriptor template:

```text
388(pk(@0/**))
```
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: pk(@0)
P2WPKH is the P2WSH witness type, so the registry key is the script hash of the P2WPKH script.
The registry key for the policy is: OP_CHECKSIG @0/**.
The BIP-388 descriptor template: 388(pk(@0/**))

```

submit: score 1.0; finish tool_calls; reason None

```text
wpkh(@0/**)
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: pk(@0)
P2WPKH is the only wrapper needed here.

```

**wp-recovery-delay-1-template**

For this Taproot wallet, Cleo is the owner and can spend without waiting. Dara is the backup: her signature works only when these coins have aged at least 1008 blocks since confirmation. Allow exactly these spending routes.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Cleo: @0
Dara: @1


Reference policy: `or(pk(@0),and(pk(@1),older(1008)))`

chat: score 1.0; finish stop; reason None

```text


Descriptor template:

```text
tr(@0/**,and_v(v:pk(@1/**),older(1008)))
```
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(pk(@1),older(1008)))
Taproot leaf Miniscript:
and_v(v:pk(@1/**),older(1008))
The Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.

```

submit: score 1.0; finish tool_calls; reason None

```text
tr(@0/**,and_v(v:pk(@1/**),older(1008)))
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(pk(@1),older(1008)))
Taproot leaf Miniscript:
and_v(v:pk(@1/**),older(1008))
The Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.

```

**wp-joint-councils-0-concrete**

Set up a Taproot reserve. Alice has an immediate owner route. The alternative route needs both council approvals: at least two of Bob, Cleo, and Dara, and at least one of Eli and Finn. Extra signatures in one council cannot replace the other council approval. The councils do not need Alice to sign. Allow exactly these spending routes.

Return a concrete descriptor using the public keys below. These are already derived for this address; no extended keys, placeholders, or wildcards are needed.
Alice: f3187214c52aae921c62a70f304798acebbd95ae826035b13c00242b76c08c9e
Bob: 5659dbceb11dd77f8061b1c17d8430f5d85911d4d82288751e725e7e5d657272
Cleo: ec3abe252c80d1350aed9599dfada023547cdf08a452dc17eaa272dbe89b6de0
Dara: a5ef5963a17ed6fb94f692a646deb7846fab06ed3596b274fbfb9127e953a5f1
Eli: bc4d9a21e29e06cec7905f8e495079004885106988d8bbba9e677bc6a740d915
Finn: 7e7fef9f1f809c5f4f4db03f73e72192aba6495c4c88883a50e9678d225399c4


Reference policy: `or(pk(@0),and(thresh(2,pk(@1),pk(@2),pk(@3)),thresh(1,pk(@4),pk(@5))))`

chat: score 0.0; finish stop; reason the script is valid but not semantically equivalent to the required spending policy

```text


Concrete descriptor:

```text
tr(f3187214c52aae921c62a70f304798acebbd95ae826035b13c00242b76c08c9e,and_v(v:multi_a(2,5659dbceb11dd77f8061b1c17d8430f5d85911d4d82288751e725e7e5d657272,ec3abe252c80d1350aed9599dfada023547cdf08a452dc17eaa272dbe89b6de0,a5ef5963a17ed6fb94f692a646deb7846fab06ed3596b274fbfb9127e953a5f1),and_v(v:pk(bc4d9a21e29e06cec7905f8e495079004885106988d8bbba9e677bc6a740d915),pk(7e7fef9f1f809c5f4f4db03f73e72192aba6495c4c88883a50e9678d225399c4))))
```
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use the supplied derived public keys directly in the descriptor.
As a policy that is: or(pk(f3187214c52aae921c62a70f304798acebbd95ae826035b13c00242b76c08c9e),and(thresh(2,pk(5659dbceb11dd77f8061b1c17d8430f5d85911d4d82288751e725e7e5d657272),pk(ec3abe252c80d1350aed9599dfada023547cdf08a452dc17eaa272dbe89b6de0),pk(a5ef5963a17ed6fb94f692a646deb7846fab06ed3596b274fbfb9127e953a5f1)),and(pk(bc4d9a21e29e06cec7905f8e495079004885106988d8bbba9e677bc6a740d915),pk(7e7fef9f1f809c5f4f4db03f73e72192aba6495c4c88883a50e9678d225399c4))))
Taproot leaf Miniscript:
and_v(v:multi_a(2,5659dbceb11dd77f8061b1c17d8430f5d85911d4d82288751e725e7e5d657272,ec3abe252c80d1350aed9599dfada023547cdf08a452dc17eaa272dbe89b6de0,a5ef5963a17ed6fb94f692a646deb7846fab06ed3596b274fbfb9127e953a5f1),and_v(v:pk(bc4d9a21e29e06cec7905f8e495079004885106988d8bbba9e677bc6a740d915),pk(7e7fef9f1f809c5f4f4db03f73e72192aba6495c4c88883a50e9678d225399c4)))
The Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.

```

submit: score 0.0; finish tool_calls; reason the script is valid but not semantically equivalent to the required spending policy

```text
tr(f3187214c52aae921c62a70f304798acebbd95ae826035b13c00242b76c08c9e,and_v(v:multi_a(2,5659dbceb11dd77f8061b1c17d8430f5d85911d4d82288751e725e7e5d657272,ec3abe252c80d1350aed9599dfada023547cdf08a452dc17eaa272dbe89b6de0,a5ef5963a17ed6fb94f692a646deb7846fab06ed3596b274fbfb9127e953a5f1),and_v(v:pk(bc4d9a21e29e06cec7905f8e495079004885106988d8bbba9e677bc6a740d915),pk(7e7fef9f1f809c5f4f4db03f73e72192aba6495c4c88883a50e9678d225399c4))))
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use the supplied derived public keys directly in the descriptor.
As a policy that is: or(pk(f3187214c52aae921c62a70f304798acebbd95ae826035b13c00242b76c08c9e),and(thresh(2,pk(5659dbceb11dd77f8061b1c17d8430f5d85911d4d82288751e725e7e5d657272),pk(ec3abe252c80d1350aed9599dfada023547cdf08a452dc17eaa272dbe89b6de0),pk(a5ef5963a17ed6fb94f692a646deb7846fab06ed3596b274fbfb9127e953a5f1)),and(pk(bc4d9a21e29e06cec7905f8e495079004885106988d8bbba9e677bc6a740d915),pk(7e7fef9f1f809c5f4f4db03f73e72192aba6495c4c88883a50e9678d225399c4))))
Taproot leaf Miniscript:
and_v(v:multi_a(2,5659dbceb11dd77f8061b1c17d8430f5d85911d4d82288751e725e7e5d657272,ec3abe252c80d1350aed9599dfada023547cdf08a452dc17eaa272dbe89b6de0,a5ef5963a17ed6fb94f692a646deb7846fab06ed3596b274fbfb9127e953a5f1),and_v(v:pk(bc4d9a21e29e06cec7905f8e495079004885106988d8bbba9e677bc6a740d915),pk(7e7fef9f1f809c5f4f4db03f73e72192aba6495c4c88883a50e9678d225399c4)))
The Taproot internal key supplies the unrestricted owner route; each leaf supplies an alternative route.

```

**wr1-owner-last-template**

For my Taproot account, Owner must keep access at all times. Backup should be able to act independently after these coins have been confirmed for at least 720 blocks. Those are the only two options.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Backup: @0
Owner: @1


Reference policy: `or(pk(@1),and(pk(@0),older(720)))`

chat: score 1.0; finish stop; reason None

```text


Descriptor template:

```text
tr(@1/**,and_v(v:pk(@0/**),older(720)))
```
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@1),and(pk(@0),older(720)))
Taproot leaf Miniscript:
and_v(v:pk(@0/**),older(720))
The Taproot internal key supplies the unrestricted owner access; each leaf supplies an alternative key path.

```

submit: score 1.0; finish tool_calls; reason None

```text
tr(@1/**,and_v(v:pk(@0/**),older(720)))
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@1),and(pk(@0),older(720)))
Taproot leaf Miniscript:
and_v(v:pk(@0/**),older(720))
The Taproot internal key supplies the unrestricted owner access; each leaf supplies an alternative key supply path.

```

**wr1-age-and-height-template**

For this Taproot wallet, Main has unrestricted access. Recovery alone may also spend, but must meet BOTH requirements: the coins are at least 400 blocks old from confirmation and the spending transaction has a block-height locktime of at least 934567. Meeting just one requirement is insufficient. No other route.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Main: @0
Recovery: @1


Reference policy: `or(pk(@0),and(pk(@1),and(older(400),after(934567))))`

chat: score 1.0; finish stop; reason None

```text


Descriptor template:

```text
tr(@0/**,and_v(v:pk(@1/**),and_v(v:older(400),after(934567))))
```
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(pk(@1),older(400),after(934567)))
Taproot leaf Miniscript:
and_v(v:pk(@1),and_v(v:older(400),after(934567)))
Taproot internal key placeholder:
@1/** and_v(v:older(400),after(934567))

```

submit: score 0.0; finish tool_calls; reason TrailingInput

```text
tr(@0/**,and_v(v:pk(@1/**),and_v(v:older(400),after(934567)))))
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),and(pk(@1),and(older(400),after(934567))))
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


Reference policy: `or(pk(@0),or(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),pk(@5))))`

chat: score 0.0; finish stop; reason the script is valid but not semantically equivalent to the required spending policy

```text


Descriptor template:

```text
tr(@0/**,and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),pk(@5/**))))
```
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),or(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),pk(@5/**)))
The Taproot internal key supplies the unrestricted owner key; each leaf supplies an alternative restriction.

```

submit: score 0.0; finish tool_calls; reason InvalidSyntax

```text
tr(@0/**,and_v(v:pk(@4),pk(@5)),multi_a(2,@1/**,@2/**,@3/**))
```

Reasoning (first 3,000 characters; full record in run directory):

```text

Use BIP-388 key placeholders with /** for receive/change derivation.
As a policy that is: or(pk(@0),or(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),pk(@5))))
Taproot leaf Miniscript:
and_v(v:pk(@4),pk(@5))
multi_a(2,@1/**,@2/**,@3/**)
The Taproot internal key supplies the unrestricted owner access; each leaf supplies an alternative path.

```
