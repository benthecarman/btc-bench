**ws1-native-single-00-template**

I want a native SegWit P2WPKH account for Bruno. A signature from Bruno is sufficient at any time. There are no other spending routes.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Bruno: @0


Policy: `pk(@0)`

P2WPKH; no custom Miniscript:



Answer:

```text
wpkh(@0/**)
```

**ws1-recovery-delay-00-template**

Please configure a Taproot reserve for Cora. Cora can always spend alone. Independently, Esme can spend alone only once the output has aged at least 6 blocks since confirmation. The owner remains unrestricted. Permit exactly those routes.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Cora: @0
Esme: @1


Policy: `or(pk(@0),and(pk(@1),older(6)))`

Taproot leaf Miniscript:

`and_v(v:pk(@1/**),older(6))`

Answer:

```text
tr(@0/**,and_v(v:pk(@1/**),older(6)))
```

**ws1-joint-councils-00-template**

Please configure a Taproot reserve for Hugo. Hugo can always spend alone. The alternative needs at least 1 of Orla, Leon, Kira, AND at least 1 of Ada and Noel. Each group must meet its own requirement; extra signatures in one group cannot replace approval from the other. Permit exactly those routes.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Hugo: @0
Orla: @1
Noel: @2
Leon: @3
Ada: @4
Kira: @5


Policy: `or(pk(@0),and(thresh(1,pk(@1),pk(@3),pk(@5)),thresh(1,pk(@4),pk(@2))))`

Taproot leaf Miniscript:

`and_v(v:multi_a(1,@1/**,@3/**,@5/**),multi_a(1,@4/**,@2/**))`

Answer:

```text
tr(@0/**,and_v(v:multi_a(1,@1/**,@3/**,@5/**),multi_a(1,@4/**,@2/**)))
```

**ws1-alternative-recovery-00-template**

Please configure a Taproot reserve for Farah. Farah can always spend alone. Kira can also spend alone at any time. A third route requires at least 1 of Esme, Ida, Cora, and is available only when the output has aged at least 6 blocks since confirmation. The team does not need either immediate signer. Permit exactly those routes.

Return a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:
Cora: @0
Kira: @1
Esme: @2
Farah: @3
Ida: @4


Policy: `or(pk(@3),or(pk(@1),and(thresh(1,pk(@2),pk(@4),pk(@0)),older(6))))`

Taproot leaf Miniscript:

`pk(@1/**)`
`and_v(v:multi_a(1,@2/**,@4/**,@0/**),older(6))`

Answer:

```text
tr(@3/**,{pk(@1/**),and_v(v:multi_a(1,@2/**,@4/**,@0/**),older(6))})
```
