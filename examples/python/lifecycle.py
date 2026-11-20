#!/usr/bin/env python3
"""
SCI Healthcare — the whole voucher lifecycle in Python.

The web app drives fund → claim → attest → settle through Freighter. This
script does the same journey with stellar-sdk for Python and local keys, so
a developer can watch the protocol run without a browser. It mirrors
scripts/seed.sh (the CLI version) but finishes the story: it waits out the
dispute window, calls `settle`, and prints the receipt the voucher
contract minted on settlement.

Usage (from examples/python/):

    python3 -m venv .venv
    .venv/bin/pip install -r requirements.txt
    ADMIN_SECRET=<seed of your deployment's admin> python3 lifecycle.py
    python3 lifecycle.py settle 7     # finish a voucher whose window has opened

Contract ids come from deployments/testnet.env, which scripts/deploy.sh
writes (env vars REGISTRY_CONTRACT_ID etc. override it; without either,
the published testnet ids from the README are used — but only the
maintainer holds that deployment's admin key, so a full run needs your
own deployment).

Keys: ADMIN_SECRET is required and must belong to the deployment's admin
(the identity deploy.sh used). CLINIC_SECRET, ATTESTER_SECRET and
FUNDER_SECRET are optional — a missing one is generated and printed so
you can re-export it next time. ISSUER_SECRET is only needed when the
deployment has no faucet contract.

Testnet only: the RPC endpoint, passphrase and friendbot are hard-coded.
"""

import os
import sys
import time
import hashlib
import secrets
from pathlib import Path
from typing import Optional, Tuple, Any, Dict, List
from dataclasses import dataclass

from dotenv import load_dotenv
from stellar_sdk import (
    Keypair,
    Network,
    SorobanServer,
    TransactionBuilder,
    Address,
    Asset,
    scval,
    xdr,
    ContractClient,
)
from stellar_sdk.exceptions import SorobanRpcErrorResponse


# ----- configuration -----

ROOT = Path(__file__).parent.parent

# Published testnet ids (README table) — used when no deployment file exists.
PUBLISHED = {
    "REGISTRY_CONTRACT_ID": "CAMU635NRFEHATJ6MTY5SI43IAZ43KHPFXAQDQAST2AHKCHLSGWHVE4V",
    "VOUCHER_CONTRACT_ID": "CBPUQN6CNJCNGPKNEXB7UM4T7RINKQB7HQI5CXEOSQ5EFRHE4G4VKPSQ",
    "RECEIPT_CONTRACT_ID": "CBCJRI2BCZBMZNX77WWDC2NGSV4ZNAN5Q23XHYNBH6UTOQMPJHZGX5C7",
    "USDC_CONTRACT_ID": "CCRLJ7FEIKTR3GBT3VQQPSIJGRBO2WPNMVFKJDP6AYGJQWECOR4QVE5M",
    "USDC_ISSUER": "GCKX5KAFVF7GLFZ3JWQGJ4F62RYYZU67PI3TACBGKVPJ73JELMI3L4KR",
}

# deployments/testnet.env written by scripts/deploy.sh, then real env, then defaults.
def load_config() -> Dict[str, str]:
    env_file = ROOT / "deployments" / "testnet.env"
    from_file: Dict[str, str] = {}
    if env_file.exists():
        for line in env_file.read_text().splitlines():
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                key, value = line.split("=", 1)
                from_file[key.strip()] = value.strip()
    return {**PUBLISHED, **from_file, **os.environ}


cfg = load_config()

REGISTRY_ID = cfg["REGISTRY_CONTRACT_ID"]
VOUCHER_ID = cfg["VOUCHER_CONTRACT_ID"]
TOKEN_ID = cfg["USDC_CONTRACT_ID"]
USDC_ISSUER = cfg["USDC_ISSUER"]
FAUCET_ID = cfg.get("FAUCET_CONTRACT_ID")

NETWORK_PASSPHRASE = Network.TESTNET_NETWORK_PASSPHRASE
RPC_URL = "https://soroban-testnet.stellar.org"
HORIZON_URL = "https://horizon-testnet.stellar.org"
EXPLORER = "https://stellar.expert/explorer/testnet/tx"

soroban_server = SorobanServer(RPC_URL)

# Demo service, as seeded by scripts/seed.sh.
SERVICE_CODE = 101
SERVICE_LABEL = "Outpatient consult"
SERVICE_PRICE = 30_000_000  # 3.00 USDC, 7 decimals
MINT_AMOUNT = 1_000_000_000  # 100 USDC when there is no faucet
# How long the script will wait for a dispute window to close (seconds).
SETTLE_WAIT_SECONDS = int(os.environ.get("SETTLE_WAIT_SECONDS", "900"))

STATUS_NAMES = ["Funded", "Claimed", "Attested", "Settled", "Disputed", "Refunded"]


@dataclass
class VoucherRecord:
    id: int
    funder: str
    provider: str
    service_code: int
    amount: int
    status: int
    created_at: int
    expires_at: int
    claimed_at: int
    attested_at: int
    dispute_deadline: int
    attester: Optional[str] = None


@dataclass
class VoucherConfig:
    admin: str
    registry: str
    receipt_book: str
    token: str
    dispute_window: int
    fee_bps: int
    fee_account: str
    attester_share_bps: int = 0


@dataclass
class ReceiptRecord:
    voucher_id: int
    provider: str
    service_code: int
    amount: int
    settled_at: int


# ----- small helpers -----

def sleep_ms(ms: int) -> None:
    time.sleep(ms / 1000.0)


def say(msg: str) -> None:
    print(f"\n==> {msg}")


def note(msg: str) -> None:
    print(f"    {msg}")


def usdc(units: int) -> str:
    """7-decimal base units → '30.00 USDC' (display only)."""
    return f"{units / 10_000_000:.2f} USDC"


def status_name(v: VoucherRecord) -> str:
    return STATUS_NAMES[v.status] if v.status < len(STATUS_NAMES) else f"unknown({v.status})"


def to_scval_address(addr: str) -> xdr.ScVal:
    return Address(addr).to_xdr_sc_val()


def to_scval_u32(v: int) -> xdr.ScVal:
    return scval.to_uint32(v)


def to_scval_u64(v: int) -> xdr.ScVal:
    return scval.to_uint64(v)


def to_scval_i128(v: int) -> xdr.ScVal:
    return scval.to_int128(v)


def to_scval_str(v: str) -> xdr.ScVal:
    return scval.to_string(v)


def to_scval_bytes32(hex_str: str) -> xdr.ScVal:
    return xdr.ScVal.scv_bytes(bytes.fromhex(hex_str))


# Recorded for the run summary and the PR.
hashes: List[Tuple[str, str]] = []


def record(step: str, hash: str) -> None:
    hashes.append((step, hash))
    note(f"{step.ljust(10)} tx {hash}")
    note(f"           {EXPLORER}/{hash}")


# ----- keys -----

def required_key(env_name: str, hint: str) -> Keypair:
    secret = os.environ.get(env_name)
    if not secret:
        raise ValueError(f"Missing {env_name}. {hint}")
    return Keypair.from_secret(secret)


def key_or_create(env_name: str, role: str) -> Keypair:
    """Uses the given secret, or generates one and prints it — the script
    friendbots whatever it creates, so a first run needs only ADMIN_SECRET."""
    secret = os.environ.get(env_name)
    if secret:
        return Keypair.from_secret(secret)
    kp = Keypair.random()
    print(f"    generated {env_name} for this run — export it to reuse the {role}:")
    print(f"      export {env_name}={kp.secret}")
    return kp


# ----- RPC plumbing (contract calls) -----

CONFIRM_TIMEOUT_MS = 60_000
POLL_INTERVAL_MS = 1_500


async def wait_for_transaction(
    hash: str,
    label: str,
) -> Any:
    deadline = time.time() * 1000 + CONFIRM_TIMEOUT_MS
    while time.time() * 1000 < deadline:
        try:
            res = await soroban_server.get_transaction(hash)
            if res.status == "SUCCESS":
                return res
            if res.status == "FAILED":
                raise RuntimeError(f"{label}: transaction failed on chain — {hash}")
        except Exception:
            pass
        await sleep_ms(POLL_INTERVAL_MS)
    raise RuntimeError(f"{label}: {hash} not confirmed in {CONFIRM_TIMEOUT_MS / 1000}s — check {EXPLORER}/{hash}")


async def sign_send_wait(
    built: TransactionBuilder,
    signer: Keypair,
    label: str,
) -> Tuple[str, Any]:
    built.sign(signer)
    sent = await soroban_server.send_transaction(built.build())
    if sent.status == "ERROR":
        raise RuntimeError(f"{label}: rejected — {sent.error_result_xdr or sent.status}")
    hash = getattr(sent, "hash", None)
    if not hash:
        raise RuntimeError(f"{label}: RPC returned {sent.status} without a transaction hash")
    result = await wait_for_transaction(hash, label)
    return hash, result.return_value if result.return_value else None


async def invoke(
    contract_id: str,
    method: str,
    args: List[xdr.ScVal],
    signer: Keypair,
    label: str,
) -> Tuple[str, Any]:
    account = await soroban_server.get_account(signer.public_key)
    contract = ContractClient(contract_id, soroban_server)
    built = TransactionBuilder(account, Network.TESTNET_NETWORK_PASSPHRASE, 100)
    built.add_operation(contract.call(method, *args))
    built.set_timeout(180)

    # Simulate first to catch contract errors before signing
    sim = await soroban_server.simulate_transaction(built.build())
    if sim.error:
        raise RuntimeError(f"{label}: {sim.error}")

    prepared = soroban_server.prepare_transaction(built.build(), sim)
    return await sign_send_wait(prepared, signer, label)


async def read(contract_id: str, method: str, args: List[xdr.ScVal] = []) -> Any:
    """Reads a view function. Costs nothing and needs no signature."""
    contract = ContractClient(contract_id, soroban_server)
    # A well-formed but unfunded account is enough to simulate a read.
    dummy_account = Keypair.random()
    built = TransactionBuilder(
        await soroban_server.get_account(dummy_account.public_key),
        Network.TESTNET_NETWORK_PASSPHRASE,
        100,
    )
    built.add_operation(contract.call(method, *args))
    built.set_timeout(30)

    sim = await soroban_server.simulate_transaction(built.build())
    if sim.error:
        raise RuntimeError(f"{method}: {sim.error}")
    if not sim.results:
        raise RuntimeError(f"{method} returned nothing")
    return sim.results[0].xdr


async def try_read(contract_id: str, method: str, args: List[xdr.ScVal] = []) -> Any:
    """For probes where 'the contract said no' is a normal answer, not a failure."""
    try:
        return await read(contract_id, method, args)
    except Exception:
        return None


# ----- accounts, trustlines, USDC -----

async def account_exists(address: str) -> bool:
    try:
        await soroban_server.get_account(address)
        return True
    except Exception as err:
        if "not found" in str(err).lower() or "404" in str(err):
            return False
        raise


async def ensure_account(kp: Keypair, role: str) -> None:
    if await account_exists(kp.public_key):
        return
    note(f"funding {role} with friendbot: {kp.public_key}")
    import aiohttp
    async with aiohttp.ClientSession() as session:
        async with session.get(f"https://friendbot.stellar.org/?addr={kp.public_key}") as res:
            if res.status != 200:
                raise RuntimeError(f"friendbot failed for {role}: {res.status} {await res.text()}")
    for _ in range(10):
        if await account_exists(kp.public_key):
            return
        await sleep_ms(1000)
    raise RuntimeError(f"{role} was not visible after friendbot funding")


async def ensure_trustline(kp: Keypair, asset: Asset, role: str) -> None:
    """A classic account needs a trustline before it can hold the SAC token."""
    account = await soroban_server.get_account(kp.public_key)
    has_trustline = any(
        hasattr(b, "asset_code") and b.asset_code == asset.code and b.asset_issuer == asset.issuer
        for b in account.balances
    )
    if has_trustline:
        return

    note(f"adding USDC trustline for {role}")
    from stellar_sdk import Operation
    tx = TransactionBuilder(account, Network.TESTNET_NETWORK_PASSPHRASE, 100)
    tx.add_operation(Operation.change_trust(asset))
    tx.set_timeout(60)
    built = tx.build()
    built.sign(kp)
    res = await soroban_server.send_transaction(built)
    if res.status != "SUCCESS":
        raise RuntimeError(f"trustline for {role} failed: {res.error_result_xdr or res.status}")
    note(f"           tx {res.hash}")


async def usdc_balance(token: str, who: str) -> Optional[int]:
    try:
        result = await try_read(token, "balance", [to_scval_address(who)])
        return int(result) if result is not None else None
    except Exception:
        return None


async def ensure_usdc(
    token: str,
    funder: Keypair,
    needed: int,
    asset: Asset,
) -> None:
    have = await usdc_balance(token, funder.public_key)
    if have is not None and have >= needed:
        note(f"funder already holds {usdc(have)}")
        return
    await ensure_trustline(funder, asset, "funder")

    if FAUCET_ID:
        try:
            hash, _ = await invoke(FAUCET_ID, "drip", [to_scval_address(funder.public_key)], funder, "faucet drip")
            record("faucet", hash)
            return
        except Exception as err:
            note(f"faucet drip unavailable ({err})")

    issuer_secret = os.environ.get("ISSUER_SECRET")
    if not issuer_secret:
        raise RuntimeError(
            f"Funder needs {usdc(needed)} but has none. Deploy the faucet "
            "(./scripts/deploy-faucet.sh) or set ISSUER_SECRET so the example can mint."
        )
    hash, _ = await invoke(
        token,
        "mint",
        [to_scval_address(funder.public_key), to_scval_i128(MINT_AMOUNT)],
        Keypair.from_secret(issuer_secret),
        "mint test USDC",
    )
    record("mint", hash)


# ----- registry setup -----

async def ensure_registry_roles(
    admin: Keypair,
    clinic: Keypair,
    attester: Keypair,
    registry: str,
) -> None:
    clinic_addr = clinic.public_key
    attester_addr = attester.public_key

    existing = await try_read(registry, "get_provider", [to_scval_address(clinic_addr)])
    if existing is None:
        hash, _ = await invoke(
            registry,
            "register_provider",
            [to_scval_address(clinic_addr), to_scval_str("Example Clinic"), to_scval_str("NG")],
            clinic,
            "register",
        )
        record("register", hash)

    active = await read(registry, "is_active_provider", [to_scval_address(clinic_addr)])
    if not active:
        # ProviderStatus: 0 Pending, 1 Active, 2 Suspended, 3 Retired (unit enum → u32),
        # then reason_code: 1 = verified (docs/protocol/trust.md).
        hash, _ = await invoke(
            registry,
            "set_provider_status",
            [
                to_scval_address(admin.public_key),
                to_scval_address(clinic_addr),
                to_scval_u32(1),
                to_scval_u32(1),
            ],
            admin,
            "activate provider",
        )
        record("activate", hash)

    price = await read(registry, "get_service_price", [to_scval_address(clinic_addr), to_scval_u32(SERVICE_CODE)])
    if price == 0:
        hash, _ = await invoke(
            registry,
            "upsert_service",
            [
                to_scval_address(clinic_addr),
                to_scval_u32(SERVICE_CODE),
                to_scval_str(SERVICE_LABEL),
                to_scval_i128(SERVICE_PRICE),
            ],
            clinic,
            "list service",
        )
        record("service", hash)

    is_attester = await read(registry, "is_attester", [to_scval_address(attester_addr)])
    if not is_attester:
        hash, _ = await invoke(
            registry,
            "add_attester",
            [to_scval_address(admin.public_key), to_scval_address(attester_addr)],
            admin,
            "add attester",
        )
        record("attester", hash)


# ----- the dispute window and settlement -----

async def settle_when_ready(
    voucher_id: int,
    signer: Keypair,
    voucher_contract: str,
) -> bool:
    voucher = await read_voucher(voucher_contract, voucher_id)
    if voucher.status == 3:
        note(f"voucher {voucher_id} is already Settled")
        return True
    if voucher.status != 2:
        raise RuntimeError(f"voucher {voucher_id} is {status_name(voucher)} — only an Attested voucher settles")

    deadline_sec = voucher.dispute_deadline
    give_up = time.time() + SETTLE_WAIT_SECONDS
    if deadline_sec - time.time() > SETTLE_WAIT_SECONDS:
        return False

    # Retry across the boundary. The deadline is ledger time and the clock
    # here is local, and testnet ledgers can lag it by tens of seconds, so a
    # `DisputeWindowOpen` rejection means "not yet" — poll until it clears.
    while time.time() < give_up:
        now_sec = int(time.time())
        if now_sec < deadline_sec - 15:
            await sleep_ms(min(5000, max(1000, (deadline_sec - now_sec) * 1000)))
            continue
        try:
            hash, _ = await invoke(voucher_contract, "settle", [to_scval_u64(voucher_id)], signer, "settle")
            record("settle", hash)
            return True
        except Exception as err:
            message = str(err)
            window_still_open = "DisputeWindowOpen" in message or "#13" in message
            if not window_still_open and now_sec >= deadline_sec + 15:
                raise
            if time.time() >= give_up:
                raise
            await sleep_ms(5000)
    return False


def print_settle_later(voucher_id: int) -> None:
    print(f"""
The dispute window is still open (deployments default to 72 hours).
This run funded, claimed and attested voucher {voucher_id}. When the window
closes, finish it from examples/python/ with:

    python3 lifecycle.py settle {voucher_id}""")


async def read_voucher(contract_id: str, voucher_id: int) -> VoucherRecord:
    result = await read(contract_id, "get_voucher", [to_scval_u64(voucher_id)])
    # The result is a tuple/vector of the voucher fields
    # Voucher: id, funder, beneficiary_ref, provider, service_code, amount,
    # status, created_at, expires_at, claimed_at, attested_at, dispute_deadline, attester
    return VoucherRecord(
        id=int(result[0]),
        funder=str(result[1]),
        provider=str(result[3]),
        service_code=int(result[4]),
        amount=int(result[5]),
        status=int(result[6]),
        created_at=int(result[7]),
        expires_at=int(result[8]),
        claimed_at=int(result[9]),
        attested_at=int(result[10]),
        dispute_deadline=int(result[11]),
        attester=str(result[12]) if result[12] else None,
    )


async def read_receipt(contract_id: str, voucher_id: int) -> ReceiptRecord:
    result = await read(contract_id, "get_receipt", [to_scval_u64(voucher_id)])
    # Receipt: voucher_id, beneficiary_ref, provider, service_code, amount, settled_at
    return ReceiptRecord(
        voucher_id=int(result[0]),
        provider=str(result[2]),
        service_code=int(result[3]),
        amount=int(result[4]),
        settled_at=int(result[5]),
    )


async def read_config(contract_id: str) -> VoucherConfig:
    result = await read(contract_id, "get_config")
    # Config: admin, registry, receipt_book, token, dispute_window, fee_bps, attester_share_bps, fee_account
    return VoucherConfig(
        admin=str(result[0]),
        registry=str(result[1]),
        receipt_book=str(result[2]),
        token=str(result[3]),
        dispute_window=int(result[4]),
        fee_bps=int(result[5]),
        attester_share_bps=int(result[6]) if len(result) > 6 else 0,
        fee_account=str(result[7]) if len(result) > 7 else str(result[6]),
    )


# ----- the run -----

async def full_lifecycle() -> None:
    admin = required_key(
        "ADMIN_SECRET",
        "It must be the seed of this deployment's admin (the identity scripts/deploy.sh used). "
        "The example runs the admin-only registry calls: activate the clinic, authorise the attester.",
    )
    clinic = key_or_create("CLINIC_SECRET", "clinic")
    attester = key_or_create("ATTESTER_SECRET", "attester")
    funder = key_or_create("FUNDER_SECRET", "funder")

    say("Configuration")
    note(f"voucher   {VOUCHER_ID}")
    note(f"registry  {REGISTRY_ID}")
    note(f"token     {TOKEN_ID}")
    note(f"rpc       {RPC_URL}")

    # Fail early with a useful message instead of a NotAuthorized deep in the run.
    registry_admin = await read(REGISTRY_ID, "get_admin")
    if registry_admin != admin.public_key:
        raise RuntimeError(
            f"Registry admin is {registry_admin}, but ADMIN_SECRET is {admin.public_key}.\n"
            "This example must point at a deployment you control. Check "
            "deployments/testnet.env, or create one with ./scripts/deploy.sh testnet."
        )
    # The voucher's own config is the source of truth for its wiring.
    config = await read_config(VOUCHER_ID)
    registry = config.registry
    token = config.token
    receipt_book = config.receipt_book
    asset = Asset("USDC", USDC_ISSUER)

    say("Accounts")
    await ensure_account(admin, "admin")
    await ensure_account(clinic, "clinic")
    await ensure_account(attester, "attester")
    await ensure_account(funder, "funder")
    note(f"admin     {admin.public_key}")
    note(f"clinic    {clinic.public_key}")
    note(f"attester  {attester.public_key}")
    note(f"funder    {funder.public_key}")

    say("Trustlines and test USDC")
    await ensure_trustline(admin, asset, "admin (fee account)")
    await ensure_trustline(clinic, asset, "clinic")
    if config.fee_account != admin.public_key:
        fee_account_has_it = (await usdc_balance(token, config.fee_account)) is not None
        if not fee_account_has_it:
            print(f"    warning: fee account {config.fee_account} has no USDC trustline; settle may fail")
    await ensure_usdc(token, funder, SERVICE_PRICE, asset)
    funder_before = await usdc_balance(token, funder.public_key)
    clinic_before = await usdc_balance(token, clinic.public_key)

    say("Registry: clinic, service, attester")
    await ensure_registry_roles(admin, clinic, attester, registry)

    say("Quote")
    fee, net = await read(VOUCHER_ID, "quote", [to_scval_i128(SERVICE_PRICE)])
    note(f"amount {usdc(SERVICE_PRICE)} — fee {usdc(int(fee))}, clinic receives {usdc(int(net))}")

    # Stands in for the client-side HMAC of a patient id; no PHI on chain.
    beneficiary_ref = hashlib.sha256(b"patient-demo-001").hexdigest()
    expires_at = int(time.time() + 30 * 24 * 60 * 60)

    say("1/4 funder escrows the voucher")
    hash, return_value = await invoke(
        VOUCHER_ID,
        "create_voucher",
        [
            to_scval_address(funder.public_key),
            to_scval_bytes32(beneficiary_ref),
            to_scval_address(clinic.public_key),
            to_scval_u32(SERVICE_CODE),
            to_scval_i128(SERVICE_PRICE),
            to_scval_u64(expires_at),
        ],
        funder,
        "fund",
    )
    voucher_id = int(return_value)
    record("fund", hash)
    note(f"voucher #{voucher_id}")

    say("2/4 clinic claims")
    hash, _ = await invoke(
        VOUCHER_ID,
        "claim",
        [to_scval_address(clinic.public_key), to_scval_u64(voucher_id)],
        clinic,
        "claim",
    )
    record("claim", hash)

    say("3/4 attester confirms delivery")
    hash, _ = await invoke(
        VOUCHER_ID,
        "attest",
        [to_scval_address(attester.public_key), to_scval_u64(voucher_id)],
        attester,
        "attest",
    )
    record("attest", hash)
    after_attest = await read_voucher(VOUCHER_ID, voucher_id)
    opens_at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(after_attest.dispute_deadline))
    note(f"status {status_name(after_attest)} — dispute window closes {opens_at}")

    say("4/4 dispute window, then settle")
    settled = await settle_when_ready(voucher_id, funder, VOUCHER_ID)
    if not settled:
        print_settle_later(voucher_id)
        summarize(hashes)
        return

    final = await read_voucher(VOUCHER_ID, voucher_id)
    receipt = await read_receipt(receipt_book, voucher_id)
    episodes_result = await read(receipt_book, "count_for", [to_scval_bytes32(beneficiary_ref)])
    episodes = int(episodes_result)
    funder_after = await usdc_balance(token, funder.public_key)
    clinic_after = await usdc_balance(token, clinic.public_key)

    say("Receipt")
    note(f"voucher #{final.id} is {status_name(final)}")
    note(f"receipt settled_at {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(receipt.settled_at))}")
    note(f"beneficiary episodes on record: {episodes}")
    if funder_before is not None and funder_after is not None:
        note(f"funder paid {usdc(funder_before - funder_after)}")
    if clinic_before is not None and clinic_after is not None:
        note(f"clinic received {usdc(clinic_after - clinic_before)}")
    summarize(hashes)


async def settle_only() -> None:
    if len(sys.argv) < 3 or not sys.argv[2].isdigit():
        raise RuntimeError('Usage: python3 lifecycle.py settle <voucher-id>')
    voucher_id = int(sys.argv[2])
    signer = key_or_create("FUNDER_SECRET", "signer (settle is permissionless)")
    await ensure_account(signer, "signer")

    voucher = await read_voucher(VOUCHER_ID, voucher_id)
    note(f"voucher #{voucher_id} is {status_name(voucher)}")
    settled = await settle_when_ready(voucher_id, signer, VOUCHER_ID)
    if not settled:
        print_settle_later(voucher_id)
        return
    config = await read_config(VOUCHER_ID)
    receipt = await read_receipt(config.receipt_book, voucher_id)
    note(f"receipt settled_at {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(receipt.settled_at))}")
    summarize(hashes)


def summarize(records: List[Tuple[str, str]]) -> None:
    say("Transactions this run")
    for step, hash in records:
        print(f"    {step.ljust(10)} {EXPLORER}/{hash}")
    print("\nLifecycle complete.")


async def main() -> None:
    print("SCI Healthcare — voucher lifecycle example (Stellar testnet)")
    if len(sys.argv) > 1 and sys.argv[1] == "settle":
        await settle_only()
    elif len(sys.argv) == 1:
        await full_lifecycle()
    else:
        raise RuntimeError(f'Unknown command "{sys.argv[1]}". Use no arguments, or: settle <voucher-id>')


if __name__ == "__main__":
    import asyncio
    load_dotenv()
    try:
        asyncio.run(main())
    except Exception as err:
        print(f"\n{err}", file=sys.stderr)
        sys.exit(1)