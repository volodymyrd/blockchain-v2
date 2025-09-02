use solana_account::{state_traits::StateMut, AccountSharedData, ReadableAccount};
use solana_pubkey::Pubkey;
use solana_rent::Rent;
use solana_sdk_ids::stake::id;
use solana_stake_interface::stake_flags::StakeFlags;
use solana_stake_interface::stake_history::Epoch;
use solana_stake_interface::state::{Authorized, Delegation, Meta, Stake, StakeStateV2};
use solana_vote_interface::state::VoteStateV3;

pub fn create_account(
    authorized: &Pubkey,
    voter_pubkey: &Pubkey,
    vote_account: &AccountSharedData,
    rent: &Rent,
    lamports: u64,
) -> AccountSharedData {
    do_create_account(
        authorized,
        voter_pubkey,
        vote_account,
        rent,
        lamports,
        Epoch::MAX,
    )
}

fn do_create_account(
    authorized: &Pubkey,
    voter_pubkey: &Pubkey,
    vote_account: &AccountSharedData,
    rent: &Rent,
    lamports: u64,
    activation_epoch: Epoch,
) -> AccountSharedData {
    let mut stake_account = AccountSharedData::new(lamports, StakeStateV2::size_of(), &id());

    let vote_state = VoteStateV3::deserialize(vote_account.data()).expect("vote_state");

    let rent_exempt_reserve = rent.minimum_balance(stake_account.data().len());

    stake_account
        .set_state(&StakeStateV2::Stake(
            Meta {
                authorized: Authorized::auto(authorized),
                rent_exempt_reserve,
                ..Meta::default()
            },
            new_stake(
                lamports - rent_exempt_reserve, // underflow is an error, is basically: assert!(lamports > rent_exempt_reserve);
                voter_pubkey,
                &vote_state,
                activation_epoch,
            ),
            StakeFlags::empty(),
        ))
        .expect("set_state");

    stake_account
}

fn new_stake(
    stake: u64,
    voter_pubkey: &Pubkey,
    vote_state: &VoteStateV3,
    activation_epoch: Epoch,
) -> Stake {
    Stake {
        delegation: Delegation::new(voter_pubkey, stake, activation_epoch),
        credits_observed: vote_state.credits(),
    }
}
