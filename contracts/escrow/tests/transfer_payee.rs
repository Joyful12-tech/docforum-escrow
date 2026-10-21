// Phase E5 tests — payee rights transfer (issue #45).
// Per AGENTS.md hard rule 2: contract logic changes need tests here.
// Payee may reassign future payout to a new address while Funded;
// requires current payee auth, rejects contract address and payer as new
// payee. A payee who has lost their key cannot use this (rotation, not
// loss).

use docforum_escrow::{EscrowContract, EscrowContractClient, EscrowStatus};
use soroban_sdk::{
    testutils::Address as _,
    token::{StellarAssetClient, TokenClient},
    Address, Env, String, Vec,
};

fn setup<'a>(env: &Env) -> (Address, TokenClient<'a>, StellarAssetClient<'a>) {
    let admin = Address::generate(env);
    let contract_id = env.register_stellar_asset_contract_v2(admin);
    let token = TokenClient::new(env, &contract_id.address());
    let asset = StellarAssetClient::new(env, &contract_id.address());
    (contract_id.address(), token, asset)
}

struct Fixture<'a> {
    client: EscrowContractClient<'a>,
    token: TokenClient<'a>,
    contract_id: Address,
    payer: Address,
    payee: Address,
    releaser: Address,
    escrow_id: u64,
    _asset: StellarAssetClient<'a>,
}

fn fund_escrow(env: &Env, amount: i128) -> Fixture<'_> {
    let (token_id, token, asset) = setup(env);
    let payer = Address::generate(env);
    let payee = Address::generate(env);
    let releaser = Address::generate(env);
    asset.mint(&payer, &1_000);

    let contract_id = env.register(EscrowContract, (Vec::<Address>::new(&env),));
    let client = EscrowContractClient::new(env, &contract_id);

    let escrow_id = client.create_escrow(
        &payer,
        &payee,
        &token_id,
        &amount,
        &String::from_str(env, "opaque-condition-ref"),
        &releaser,
        &None,
        &false,
        &None,
    );

    Fixture {
        client,
        token,
        contract_id,
        payer,
        payee,
        releaser,
        escrow_id,
        _asset: asset,
    }
}

#[test]
fn transfer_payee_allows_current_payee_to_reassign_while_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let f = fund_escrow(&env, 300);
    let new_payee = Address::generate(&env);

    f.client.transfer_payee(&f.escrow_id, &f.payee, &new_payee);

    // Future release goes to the new payee
    f.client.release(&f.escrow_id, &f.releaser);
    assert_eq!(f.token.balance(&new_payee), 300);
    assert_eq!(f.token.balance(&f.payee), 0);
    assert_eq!(f.client.get_status(&f.escrow_id), EscrowStatus::Released);
}

#[test]
fn transfer_payee_requires_current_payee_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let f = fund_escrow(&env, 300);
    let stranger = Address::generate(&env);
    let new_payee = Address::generate(&env);

    // Stranger cannot transfer even if they pass stranger as current_payee
    let result = f.client.try_transfer_payee(&f.escrow_id, &stranger, &new_payee);
    assert!(result.is_err());
    assert_eq!(f.client.get_status(&f.escrow_id), EscrowStatus::Funded);

    // Releaser cannot transfer
    let result = f.client.try_transfer_payee(&f.escrow_id, &f.releaser, &new_payee);
    assert!(result.is_err());

    // Payer cannot transfer
    let result = f.client.try_transfer_payee(&f.escrow_id, &f.payer, &new_payee);
    assert!(result.is_err());
}

#[test]
fn transfer_payee_rejects_contract_address_as_new_payee() {
    let env = Env::default();
    env.mock_all_auths();
    let f = fund_escrow(&env, 300);

    let result = f
        .client
        .try_transfer_payee(&f.escrow_id, &f.payee, &f.contract_id);
    assert!(result.is_err());
    assert_eq!(f.client.get_status(&f.escrow_id), EscrowStatus::Funded);
}

#[test]
fn transfer_payee_rejects_payer_as_new_payee() {
    let env = Env::default();
    env.mock_all_auths();
    let f = fund_escrow(&env, 300);

    let result = f
        .client
        .try_transfer_payee(&f.escrow_id, &f.payee, &f.payer);
    assert!(result.is_err());
    assert_eq!(f.client.get_status(&f.escrow_id), EscrowStatus::Funded);
}

#[test]
fn transfer_payee_only_while_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let f = fund_escrow(&env, 300);
    let new_payee = Address::generate(&env);

    // Cannot transfer after release
    f.client.release(&f.escrow_id, &f.releaser);
    let result = f.client.try_transfer_payee(&f.escrow_id, &f.payee, &new_payee);
    assert!(result.is_err());

    // Cannot transfer after refund
    let env2 = Env::default();
    env2.mock_all_auths();
    let f2 = fund_escrow(&env2, 300);
    let new_payee2 = Address::generate(&env2);
    f2.client.refund(&f2.escrow_id, &f2.releaser);
    let result = f2.client.try_transfer_payee(&f2.escrow_id, &f2.payee, &new_payee2);
    assert!(result.is_err());
}

#[test]
fn transfer_payee_can_be_transferred_multiple_times_by_current_payee() {
    let env = Env::default();
    env.mock_all_auths();
    let f = fund_escrow(&env, 300);
    let new_payee1 = Address::generate(&env);
    let new_payee2 = Address::generate(&env);

    // First transfer
    f.client.transfer_payee(&f.escrow_id, &f.payee, &new_payee1);
    // New payee transfers again
    f.client
        .transfer_payee(&f.escrow_id, &new_payee1, &new_payee2);

    f.client.release(&f.escrow_id, &f.releaser);
    assert_eq!(f.token.balance(&new_payee2), 300);
    assert_eq!(f.token.balance(&new_payee1), 0);
    assert_eq!(f.token.balance(&f.payee), 0);
}
