use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_units::parse_near;

pub const NAME: &str = "Theia Collection 1";
pub const SYMBOL: &str = "TCN";
pub const NFT_BASE_URI: &str = "https://ipfs.io/ipfs/QmUDqczgXxZ7exQ9znjZRB1CCvEmQ5FZchatueZXWnIkly/";
pub const NFT_BLANK_URI: &str = "https://ipfs.io/ipfs/QmZRBnIklexQCvEmQxZ1CDqczgXcy7hatu9eZXW5FZUznj";
pub const NFT_MAX_SUPPLY: u128 = 1000u128;
pub const PRE_MINT_AMOUNT: u128 = 2u128;
pub const FUND_THRESHOLD: u128 = parse_near!("200 N");
pub const PROTOCOL_FEE: u16 = 1u16; // 1%
pub const FINDER_FEE: u16 = 1u16; // 1%

pub const ONE_HOUR: u128 = 3600u128 * 1000;
pub const TOW_HOURS: u128 = 3600u128 * 2 * 1000;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Eq, PartialEq, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub enum CurveType {
    Horizontal,
    Linear,
    Sigmoidal,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, PartialEq, Clone, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct CurveArgs {
    pub arg_a: Option<u128>,
    pub arg_b: Option<u128>,
    pub arg_c: Option<u128>,
    pub arg_d: Option<u128>,
}
