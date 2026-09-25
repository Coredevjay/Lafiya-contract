# Lafiya 🔏

[![Built on Stellar](https://img.shields.io/badge/Built%20on-Stellar-blue?logo=stellar)](https://stellar.org)
[![Soroban Smart Contracts](https://img.shields.io/badge/Smart%20Contracts-Soroban-purple)](https://soroban.stellar.org)
[![Status](https://img.shields.io/badge/status-pre--alpha-orange)]()
[![Network](https://img.shields.io/badge/network-testnet-lightgrey)]()
[![CI](https://github.com/Lafiya-xyz/Lafiya-contract/actions/workflows/ci.yml/badge.svg)](https://github.com/Lafiya-xyz/Lafiya-contract/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Docs](https://github.com/Lafiya-xyz/Lafiya-contract/actions/workflows/docs.yml/badge.svg)](https://lafiya-xyz.github.io/Lafiya-contract/main/)

Soroban smart contracts for Lafiya's on-chain trust layer — an attestation registry and attester allowlist that let a health worker's verification of an emergency health record be checked cryptographically, without the underlying health data ever touching the blockchain.

**Your vitals, verified. When you can't speak, Lafiya does.**

*Lafiya* is Hausa for health, safety, and wellbeing.

> **Status:** Pre-alpha · Stellar **testnet** · not yet audited · not a medical device. See [Disclaimer](#disclaimer).

## Overview

Lafiya is a free, patient-owned emergency health card. The handful of facts that change how you are treated in an emergency (blood group, genotype, allergies, current medications, chronic conditions) travel with you as a scannable QR code, and a health worker's verification of them can be **cryptographically checked** on the spot.

This repository holds only the Soroban smart contract layer:

- **`attester-registry`**: the admin-managed allowlist of health workers (*attesters*).
- **`attestation-registry`**: records that an allowlisted attester verified a record commitment. It stores only an opaque hash; health data never goes on-chain.
- **`multisig-account`**: an N-of-M custom account intended as the registries' admin.

## Documentation

The full documentation lives at **[https://lafiya-xyz.github.io/Lafiya-contract/main/](https://lafiya-xyz.github.io/Lafiya-contract/main/)** (versioned per release, with search).

| Section | What's there |
| --- | --- |
| [Overview](https://lafiya-xyz.github.io/Lafiya-contract/main/overview.html) | Problem, architecture, contract layer, roadmap (the previous README) |
| [Concepts](https://lafiya-xyz.github.io/Lafiya-contract/main/glossary.html) | Glossary, trust model, data flow |
| [Contracts](https://lafiya-xyz.github.io/Lafiya-contract/main/reference/contracts/attester-registry.html) | Generated per-contract reference: functions, errors, events |
| [Integrate](https://lafiya-xyz.github.io/Lafiya-contract/main/typescript-bindings.html) | TypeScript bindings, commitment scheme, verifier integration |
| [Operate](https://lafiya-xyz.github.io/Lafiya-contract/main/runbooks/contract-upgrade.html) | Runbooks, generated CLI reference, local sandbox |
| [Decisions](https://lafiya-xyz.github.io/Lafiya-contract/main/adr/README.html) | Architecture Decision Records |
| [API](https://lafiya-xyz.github.io/Lafiya-contract/main/api.html) | rustdoc |

In the repository: [glossary](docs/glossary.md) · [ADRs](docs/adr/README.md) · [runbooks](docs/runbooks/) · [local sandbox](sandbox/README.md) · [TLA+ models](verification/tla/README.md)

## Getting started

```bash
git clone https://github.com/Lafiya-xyz/Lafiya-contract.git
cd Lafiya-contract
make check                                    # fmt-check + clippy + test + wasm build
cargo xtask sandbox up --scenario edge-cases  # seeded local chain (Docker + stellar CLI)
```

Build the docs site locally (needs [mdBook](https://rust-lang.github.io/mdBook/)):

```bash
cargo xtask docs && mdbook serve docs
```

## Disclaimer

Lafiya is an information aid, **not a medical device** and **not a substitute for professional medical judgment**. Verified indicators reflect that a record was attested by a registered health worker; they are not a clinical guarantee. Treatment decisions remain the responsibility of the attending clinician.

## Security

Found a vulnerability? Please don't open a public issue. See [SECURITY.md](SECURITY.md) for how to report it privately.

## Contributing and license

See [CONTRIBUTING.md](CONTRIBUTING.md). Licensed under [MIT](LICENSE).
