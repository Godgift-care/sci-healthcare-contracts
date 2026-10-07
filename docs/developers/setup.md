---
title: Local setup
nav_order: 9
---

# Local setup

Three repositories. Build them in this order — the app needs contract addresses that only exist after deployment.

| Repo | What it is |
| --- | --- |
| [sci-healthcare-contracts](https://github.com/Godgift-care/sci-healthcare-contracts) | Soroban contracts (Rust) |
| [sci-healthcare-backend](https://github.com/Godgift-care/sci-healthcare-backend) | Event indexer and read API |
| [sci-healthcare-frontend](https://github.com/Godgift-care/sci-healthcare-frontend) | Web app |

## Prerequisites

| Tool | Version |
| --- | --- |
| Rust | 1.96.0 |
| wasm target | `wasm32v1-none` |
| Stellar CLI | 27.0.0+ |
| Node | 22 LTS |

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install 1.96.0
rustup target add wasm32v1-none
cargo install --locked stellar-cli
```

## 1. Contracts

```bash
git clone https://github.com/Godgift-care/sci-healthcare-contracts
cd sci-healthcare-contracts

cargo test                                    # 69 tests
cargo build --target wasm32v1-none --release
```

Create and fund identities, then deploy:

```bash
for id in sci-admin sci-issuer sci-clinic sci-attester sci-funder; do
  stellar keys generate --network testnet --fund $id
done

./scripts/deploy.sh testnet
./scripts/seed.sh testnet
```

`deploy.sh` deploys in dependency order — registry and receipt first, then voucher — initialises each, points the receipt book at the voucher contract, and writes every address to `deployments/testnet.env`.

`seed.sh` creates a demo clinic with three services, authorises an attester, and funds two vouchers.

### The testnet dispute window

`deploy.sh` defaults to a **72-hour** dispute window. That makes a demo impossible to finish, so deploy with a short one for local work:

```bash
DISPUTE_WINDOW=60 ./scripts/deploy.sh testnet
```

## 2. Backend

```bash
git clone https://github.com/Godgift-care/sci-healthcare-backend
cd sci-healthcare-backend
npm install
cp .env.example .env
```

Copy the addresses from `deployments/testnet.env` into `.env`, then:

```bash
npx prisma generate
npx prisma db push
npm run dev          # http://localhost:8080
```

> **Set `INDEXER_START_LEDGER`.** Left at `0` the indexer starts at the current tip and silently skips everything that happened before, so your seeded data never appears. Set it to a ledger just before you deployed:
>
> ```bash
> curl -s -X POST https://soroban-testnet.stellar.org \
>   -H 'Content-Type: application/json' \
>   -d '{"jsonrpc":"2.0","id":1,"method":"getLatestLedger"}'
> ```

Check it caught up:

```bash
curl -s localhost:8080/stats | jq
curl -s localhost:8080/ready | jq
```

## 3. Frontend

```bash
git clone https://github.com/Godgift-care/sci-healthcare-frontend
cd sci-healthcare-frontend
npm install
cp .env.example .env.local
```

Fill in the same addresses with the `NEXT_PUBLIC_` prefix, then:

```bash
npm run dev          # http://localhost:3000
```

## Things that go wrong

**`DATABASE_URL` creates `prisma/prisma/dev.db`.** Prisma resolves a relative SQLite path against the schema file, not the project root. Use `file:./dev.db`, not `file:./prisma/dev.db`.

**The app still calls localhost after deploying.** `NEXT_PUBLIC_*` is inlined at build time. Setting it in the host's runtime environment does nothing — it has to be in the build environment, and you have to rebuild.

**Indexed rows contain the string `"undefined"`.** A contract event field marked `#[topic]` is published in the topic list, not the data map. Reading it off the data map yields `undefined`, which stringifies. Check `TOPIC_FIELDS` in `src/stellar/events.ts` matches the contract.

**`Error(Contract, #13)` when settling.** The dispute window is still open. Wait it out, or redeploy with a shorter `DISPUTE_WINDOW`.

**Enum arguments rejected by the CLI.** Unit enums with explicit discriminants take the integer, not the name: `--status 1`, not `--status Active`.

**`BytesN` arguments rejected.** These take raw hex, not JSON. Pass the 64 hex characters directly.
