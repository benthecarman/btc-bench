Eight authored training scenarios, each paired as a fresh request and a repair, with template and concrete-key output. Both chat and submit SFT rows are prepared. No training has started.

All correct targets and final chat responses pass the wallet verifier. Every bad draft earns zero. Explicit policies and Miniscript bodies are checked separately. Human review must still check that the requests express the intended contract.

These are targeted training examples, not a new transfer benchmark. All derivatives of each scenario share a group.

**wrp1-one-derivation-template-repair**

I am setting up a native SegWit account for Nina. She alone can spend, with no delay or recovery route.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Nina: @0

I tried this, but it may be wrong. Please fix it to match the requirements above:

```text
wpkh(@0/**/**)
```

Correction: A template key needs one receive/change derivation suffix. The shorthand /** already means /<0;1>/*. Do not append a second suffix.

Spending policy:

```text
pk(@0)
```

Miniscript (no script body is needed for this output).

Correct descriptor:

```text
wpkh(@0/**)
```

**wrp1-no-comment-suffix-template-repair**

Make a Taproot account for Omar. His key is the only spending route, and he can spend at any time.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Omar: @0

I tried this, but it may be wrong. Please fix it to match the requirements above:

```text
tr(@0/**)#Taproot
```

Correction: A # suffix is not a free-form label. Return the descriptor without the invented #Taproot suffix; no script tree is needed.

Spending policy:

```text
pk(@0)
```

Miniscript (no script body is needed for this output).

Correct descriptor:

```text
tr(@0/**)
```

**wrp1-single-leaf-template-repair**

I need a Taproot reserve. Ravi can spend alone whenever needed. Alternatively, any two of Pia, Quinn, and Suri can spend together without Ravi. Those are the only routes.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Pia: @0
Quinn: @1
Ravi: @2
Suri: @3

I tried this, but it may be wrong. Please fix it to match the requirements above:

```text
tr(@2/**,{multi_a(2,@0/**,@1/**,@3/**)})
```

Correction: The team is one script leaf. Place it directly after the internal key. Braces join two tree branches; they do not wrap a single leaf.

Spending policy:

```text
or(pk(@2),thresh(2,pk(@0),pk(@1),pk(@3)))
```

Miniscript:

```text
multi_a(2,@0/**,@1/**,@3/**)
```

Correct descriptor:

```text
tr(@2/**,multi_a(2,@0/**,@1/**,@3/**))
```

**wrp1-every-template-key-template-repair**

For my Taproot savings account, Uma can spend at any time. Tess can recover the money once the output has aged at least 720 blocks since confirmation, without Uma. Keep both routes available after that.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Tess: @0
Uma: @1

I tried this, but it may be wrong. Please fix it to match the requirements above:

```text
tr(@1/**,and_v(v:pk(@0),older(720)))
```

Correction: The recovery key needs its receive/change derivation suffix too. The internal key and the leaf key are both template keys. The 720-block delay applies only to Tess.

Spending policy:

```text
or(pk(@1),and(pk(@0),older(720)))
```

Miniscript:

```text
and_v(v:pk(@0/**),older(720))
```

Correct descriptor:

```text
tr(@1/**,and_v(v:pk(@0/**),older(720)))
```

**wrp1-and-v-type-template-repair**

Create a native SegWit script account that requires both Vera and Will to sign. Neither can spend alone. There is no time restriction or alternate route.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Vera: @0
Will: @1

I tried this, but it may be wrong. Please fix it to match the requirements above:

```text
wsh(and_v(pk(@0/**),pk(@1/**)))
```

Correction: and_v requires a V-type first argument. The v: wrapper verifies the first signature result; the second pk remains the final Boolean result.

Spending policy:

```text
and(pk(@0),pk(@1))
```

Miniscript:

```text
and_v(v:pk(@0/**),pk(@1/**))
```

Correct descriptor:

```text
wsh(and_v(v:pk(@0/**),pk(@1/**)))
```

**wrp1-or-b-type-template-repair**

I want a native SegWit script account where either Xena or Yuri can spend alone. There is no waiting period and neither needs the other signature.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Xena: @0
Yuri: @1

I tried this, but it may be wrong. Please fix it to match the requirements above:

```text
wsh(or_b(pk(@0/**),pk(@1/**)))
```

Correction: or_b requires B and W arguments. The s: wrapper turns the second pk into the required W form. Either signature is still sufficient.

Spending policy:

```text
or(pk(@0),pk(@1))
```

Miniscript:

```text
or_b(pk(@0/**),s:pk(@1/**))
```

Correct descriptor:

```text
wsh(or_b(pk(@0/**),s:pk(@1/**)))
```

**wrp1-separate-councils-template-repair**

Set up a Taproot reserve with Faye as the owner. Faye can spend alone at any time. Without Faye, a withdrawal needs at least one of Asha, Bea, and Chen AND both Dale and Enid. Extra signatures from the first group cannot replace Dale or Enid. Allow exactly these routes.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Asha: @0
Bea: @1
Chen: @2
Dale: @3
Enid: @4
Faye: @5

I tried this, but it may be wrong. Please fix it to match the requirements above:

```text
tr(@5/**,multi_a(3,@0/**,@1/**,@2/**,@3/**,@4/**))
```

Correction: Keep the two group requirements separate. A single three-of-five threshold would let Asha, Bea, and Chen spend without Dale or Enid.

Spending policy:

```text
or(pk(@5),and(thresh(1,pk(@0),pk(@1),pk(@2)),and(pk(@3),pk(@4))))
```

Miniscript:

```text
and_v(v:multi_a(1,@0/**,@1/**,@2/**),multi_a(2,@3/**,@4/**))
```

Correct descriptor:

```text
tr(@5/**,and_v(v:multi_a(1,@0/**,@1/**,@2/**),multi_a(2,@3/**,@4/**)))
```

**wrp1-independent-routes-template-repair**

Build a Taproot wallet with three ways to spend. Gita can spend alone immediately. Any two of Hale, Imani, and Jules can also spend immediately without Gita. Separately, Kian can recover the money once the output is at least 576 blocks old from confirmation, without anyone else. Each route is independent; permit no others.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Gita: @0
Hale: @1
Imani: @2
Jules: @3
Kian: @4

I tried this, but it may be wrong. Please fix it to match the requirements above:

```text
tr(@0/**,and_v(v:multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),older(576))))
```

Correction: Use two alternative script leaves. Joining the team and recovery conditions with and_v would require both, so neither promised independent route would work.

Spending policy:

```text
or(pk(@0),or(thresh(2,pk(@1),pk(@2),pk(@3)),and(pk(@4),older(576))))
```

Miniscript:

```text
multi_a(2,@1/**,@2/**,@3/**)
and_v(v:pk(@4/**),older(576))
```

Correct descriptor:

```text
tr(@0/**,{multi_a(2,@1/**,@2/**,@3/**),and_v(v:pk(@4/**),older(576))})
```

Reference syntax: [BIP-388](https://github.com/bitcoin/bips/blob/master/bip-0388.mediawiki) and [Miniscript](https://bitcoin.sipa.be/miniscript/). Full template/concrete requests are in `wallet-repairs-v1-review.json`.
