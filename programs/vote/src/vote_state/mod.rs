use solana_account::{AccountSharedData, WritableAccount};
use solana_clock::Clock;
use solana_pubkey::Pubkey;
use solana_sdk_ids::vote::id;
use solana_vote_interface::state::{VoteInit, VoteStateV3, VoteStateVersions};

pub fn create_account_with_authorized(
    node_pubkey: &Pubkey,
    authorized_voter: &Pubkey,
    authorized_withdrawer: &Pubkey,
    commission: u8,
    lamports: u64,
) -> AccountSharedData {
    let mut vote_account = AccountSharedData::new(lamports, VoteStateV3::size_of(), &id());

    let vote_state = VoteStateV3::new(
        &VoteInit {
            node_pubkey: *node_pubkey,
            authorized_voter: *authorized_voter,
            authorized_withdrawer: *authorized_withdrawer,
            commission,
        },
        &Clock::default(),
    );

    VoteStateV3::serialize(
        &VoteStateVersions::V3(Box::new(vote_state)),
        vote_account.data_as_mut_slice(),
    )
    .unwrap();

    vote_account
}
