mod utils;
mod errors;
mod views;
mod curves;
mod owner;
mod validates;
mod pause;
mod token_receiver;

use near_contract_standards::non_fungible_token::TokenId;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::env::STORAGE_PRICE_PER_BYTE;
use near_sdk::{env, near_bindgen, AccountId, Balance, PanicOnDefault, Promise, Gas, is_promise_success, log, require};
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use crate::errors::*;
use crate::utils::*;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    /// Owner of contract
    owner_id: AccountId,
    /// Protocol account
    treasury_id: AccountId,
    /// Protocol fee percent
    treasury_fee: u32,
    /// Finder account
    finder_id: Option<AccountId>,
    /// Finder fee percent
    finder_fee: u32,
    /// Project token type
    project_token_type: ProjectTokenType,
    /// Project token id
    project_token_id: Option<AccountId>,
    /// Proxy token id
    proxy_token_id: Option<AccountId>,
    /// Funding target amount
    fund_threshold: Balance,
    /// Start timestamp
    start_timestamp: u64,
    /// Threshold timestamp
    tp_timestamp: u64,
    /// Buffer period
    buffer_period: u64,
    /// Conversion period
    conversion_period: u64,
    /// Stable coin
    stable_coin_id: AccountId,
    /// Stable coin decimals
    stable_coin_decimals: u8,
    /// Total fund amount
    total_fund_amount: Balance,
    /// Pre-mint amount
    pre_mint_amount: Balance,
    /// Amount of converted proxy token
    converted_proxy_token_amount: Balance,
    /// Auction curve type
    curve_type: CurveType,
    /// Auction curve args
    curve_args: CurveArgs,
    /// Running state
    state: RunningState,
    /// Closed
    is_closed: bool,
}

// 
const MIN_STORAGE_NON_FUNGIBLE_TOKEN: Balance = 7_000_000_000_000_000_000_000_000;
//1.1Ⓝ
const MIN_STORAGE_FUNGIBLE_TOKEN: Balance = 4_100_000_000_000_000_000_000_000;
const MIN_STORAGE_PROXY_TOKEN : Balance = 7_500_000_000_000_000_000_000_000;
const DEPOSIT_ONE_PROXY_TOKEN_MINT: Balance = 4_100_000_000_000_000_000_000_000;
const DEPOSIT_ONE_NFT_MINT: Balance = 638 * STORAGE_PRICE_PER_BYTE;
const NO_DEPOSIT: Balance = 0u128;
const ONE_YOCTO: Balance = 1u128;
const TGAS: u64 = 1_000_000_000_000;

const PROXY_TOKEN_CODE: &[u8] = include_bytes!("../../target/wasm32-unknown-unknown/release/proxy_token.wasm");
const NFT_COLLECTION_CODE: &[u8] = include_bytes!("../../target/wasm32-unknown-unknown/release/nft_collection.wasm");
const FUNGIBLE_TOKEN_CODE: &[u8] = include_bytes!("../../target/wasm32-unknown-unknown/release/ft_token.wasm");

#[near_bindgen]
impl Contract {
    /// Initialize the contract
    #[init]
    pub fn new(owner_id: AccountId, stable_coin_id: AccountId, stable_coin_decimals: u8, curve_type: CurveType, curve_args: CurveArgs, treasury_id: AccountId) -> Self {
        assert!(!env::state_exists(), "{}", ERR08_ALREADY_INITIALIZED);

        Self {
            owner_id,
            treasury_id,
            treasury_fee: 100,  // 1%
            finder_id: None,
            finder_fee: 100,    // 1%
            project_token_type: ProjectTokenType::NonFungible,
            project_token_id: None,
            proxy_token_id: None,
            fund_threshold: 0,
            start_timestamp: 0,
            tp_timestamp: 0,
            buffer_period: 0,
            conversion_period: 0,
            stable_coin_id,
            stable_coin_decimals,
            total_fund_amount: 0,
            pre_mint_amount: 0,
            converted_proxy_token_amount: 0,
            curve_type,
            curve_args,
            state: RunningState::Running,
            is_closed: false,
        }
    }

    /// Active NFT project
    pub fn active_nft_project(&mut self, name: String, symbol: String, base_uri: String, blank_media_uri: String, max_supply: Balance, finder_id: AccountId, pre_mint_amount: Balance, fund_threshold: Balance, buffer_period: u64, conversion_period: u64) -> Promise {
        self.assert_owner();
        assert_eq!(self.is_closed, false, "{}", ERR013_ALREADY_CLOSED);
        assert!(name.len() > 2, "{}", ERR00_INVALID_NAME);
        assert!(symbol.len() < 13 && symbol.len() > 2, "{}", ERR01_INVALID_SYMBOL);
        assert!(base_uri.len() > 0, "{}", ERR02_INVALID_COLLECTION_BASE_URI);
        assert!(blank_media_uri.len() > 0, "{}", ERR03_INVALID_BLANK_URI);
        assert!(max_supply > 0, "{}", ERR04_INVALID_MAX_SUPPLY);
        assert!(fund_threshold > 0, "{}", ERR05_INVALID_FUNDING_TARGET);
        assert!(conversion_period >= 86400, "{}", ERR06_INVALID_CONVERSION_PERIOD);

        self.finder_id = Some(finder_id);
        self.fund_threshold = fund_threshold;
        self.pre_mint_amount = pre_mint_amount;
        self.buffer_period = buffer_period;
        self.conversion_period = conversion_period;
        self.start_timestamp = env::block_timestamp();
        self.project_token_type = ProjectTokenType::NonFungible;

        let mut token_suffix = name.clone().to_lowercase();
        token_suffix.retain(|c| !c.is_whitespace());
        let project_token_id = AccountId::new_unchecked(format!("{}.{}", token_suffix, env::current_account_id()));
        let proxy_token_id = AccountId::new_unchecked(format!("p{}.{}", token_suffix, env::current_account_id()));

        // deploy non-fungible token
        let project_token_promise = Promise::new(project_token_id.clone())
            .create_account()
            .transfer(MIN_STORAGE_NON_FUNGIBLE_TOKEN)
            .deploy_contract(NFT_COLLECTION_CODE.to_vec())
            .function_call(
                "new".to_string(),
                json!({
                    "owner_id": env::current_account_id(),
                    "name": name.clone(),
                    "symbol": symbol.clone(),
                    "base_uri": base_uri,
                    "max_supply": U128::from(max_supply)
                }).to_string().as_bytes().to_vec(),
                NO_DEPOSIT,
                Gas(5 * TGAS)
            );

        // deploy proxy token
        let proxy_token_promise = Promise::new(proxy_token_id.clone())
            .create_account()
            .transfer(MIN_STORAGE_PROXY_TOKEN)
            .deploy_contract(PROXY_TOKEN_CODE.to_vec())
            .function_call(
                "new".to_string(),
                json!({
                        "owner_id": env::current_account_id(),
                        "name": name,
                        "symbol": symbol,
                        "blank_media_uri": blank_media_uri,
                        "max_supply": U128::from(max_supply)
                    }).to_string().as_bytes().to_vec(),
                NO_DEPOSIT,
                Gas(5 * TGAS)
            )
            .function_call(
                "mt_mint".to_string(),
                json!({
                        "receiver_id": self.owner_id.clone(),
                        "amount": U128::from(pre_mint_amount)
                    }).to_string().as_bytes().to_vec(),
                    DEPOSIT_ONE_PROXY_TOKEN_MINT,
                Gas(5 * TGAS)
            );

        project_token_promise
            .and(proxy_token_promise)
            .then(
                ext_self::ext(env::current_account_id()).on_activate(project_token_id, proxy_token_id)
            )
    }

    /// Active FT project
    pub fn active_ft_project(&mut self, name: String, symbol: String, blank_media_uri: String, max_supply: Balance, finder_id: AccountId, pre_mint_amount: Balance, fund_threshold: Balance, buffer_period: u64, conversion_period: u64) -> Promise {
        self.assert_owner();
        assert_eq!(self.is_closed, false, "{}", ERR013_ALREADY_CLOSED);
        assert!(name.len() > 2, "{}", ERR00_INVALID_NAME);
        assert!(symbol.len() < 13 && symbol.len() > 2, "{}", ERR01_INVALID_SYMBOL);
        assert!(blank_media_uri.len() > 0, "{}", ERR03_INVALID_BLANK_URI);
        assert!(max_supply > 0, "{}", ERR04_INVALID_MAX_SUPPLY);
        assert!(fund_threshold > 0, "{}", ERR05_INVALID_FUNDING_TARGET);
        assert!(conversion_period >= 86400, "{}", ERR06_INVALID_CONVERSION_PERIOD);

        self.finder_id = Some(finder_id);
        self.fund_threshold = fund_threshold;
        self.pre_mint_amount = pre_mint_amount;
        self.buffer_period = buffer_period;
        self.conversion_period = conversion_period;
        self.start_timestamp = env::block_timestamp();
        self.project_token_type = ProjectTokenType::Fungible;

        let mut token_suffix = name.clone().to_lowercase();
        token_suffix.retain(|c| !c.is_whitespace());
        let project_token_id = AccountId::new_unchecked(format!("{}.{}", token_suffix, env::current_account_id()));
        let proxy_token_id = AccountId::new_unchecked(format!("p{}.{}", token_suffix, env::current_account_id()));

        // deploy fungible token
        let project_token_promise = Promise::new(project_token_id.clone())
            .create_account()
            .transfer(MIN_STORAGE_FUNGIBLE_TOKEN)
            .deploy_contract(FUNGIBLE_TOKEN_CODE.to_vec())
            .function_call(
                "new".to_string(),
                json!({
                    "owner_id": env::current_account_id(),
                    "name": name.clone(),
                    "symbol": symbol.clone()
                }).to_string().as_bytes().to_vec(),
                NO_DEPOSIT,
                Gas(5 * TGAS)
            );

        // deploy proxy token
        let proxy_token_promise = Promise::new(proxy_token_id.clone())
            .create_account()
            .transfer(MIN_STORAGE_PROXY_TOKEN)
            .deploy_contract(PROXY_TOKEN_CODE.to_vec())
            .function_call(
                "new".to_string(),
                json!({
                        "owner_id": env::current_account_id(),
                        "name": name,
                        "symbol": symbol,
                        "blank_media_uri": blank_media_uri,
                        "max_supply": U128::from(max_supply)
                    }).to_string().as_bytes().to_vec(),
                NO_DEPOSIT,
                Gas(5 * TGAS)
            )
            .function_call(
                "mt_mint".to_string(),
                json!({
                        "receiver_id": self.owner_id.clone(),
                        "amount": U128::from(pre_mint_amount)
                    }).to_string().as_bytes().to_vec(),
                    DEPOSIT_ONE_PROXY_TOKEN_MINT * pre_mint_amount,
                Gas(5 * TGAS)
            );

        project_token_promise.and(proxy_token_promise).then(
            ext_self::ext(env::current_account_id()).on_activate(project_token_id, proxy_token_id)
        )
    }


    /// Callback after project token was created
    #[private]
    pub fn on_activate(
        &mut self,
        project_token_id: AccountId,
        proxy_token_id: AccountId
    ) {
        require!(env::promise_results_count() == 2, "Contract expected a result on the callback");

        if is_promise_ok(env::promise_result(0)) && is_promise_ok(env::promise_result(1)) {
            self.project_token_id = Some(project_token_id.clone());
            self.proxy_token_id = Some(proxy_token_id.clone());
            log!("Activated {} {}", proxy_token_id.to_string(), project_token_id.to_string());
        } else {
            env::panic_str(ERR015_ACTIVATE_FAILED);
        }
    }

    /// buy proxy token
    pub(crate) fn buy(&mut self, from: AccountId, amount: U128, coin_amount: U128) -> Promise {
        self.assert_not_paused();
        self.assert_is_ongoing();

        let cal_coin_amount = self.calculate_buy_proxy_token(amount);
        assert!(coin_amount.0 >= cal_coin_amount, "{}", ERR07_INSUFFICIENT_FUND);

        let treasury_fee_amount = cal_coin_amount
            .checked_mul(self.treasury_fee as u128)
            .unwrap()
            .checked_div(FEE_DIVISOR as u128)
            .unwrap();

        let reserve_fund_amount = cal_coin_amount.checked_sub(treasury_fee_amount).unwrap();

        self.total_fund_amount = self.total_fund_amount
            .checked_add(reserve_fund_amount)
            .unwrap();

        if self.tp_timestamp == 0 && self.total_fund_amount >= self.fund_threshold {
            self.tp_timestamp = env::block_timestamp();
        }

        // Transfer stable coin to treasury
        let treasury_promise = ext_fungible_token::ext(self.stable_coin_id.clone())
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .with_attached_deposit(ONE_YOCTO)
            .ft_transfer(
                self.treasury_id.clone(),
                U128::from(treasury_fee_amount),
                None,
            );

        //Mint proxy token to customer
        let proxy_token_mint_promise = ext_proxy_token::ext(self.proxy_token_id.clone().unwrap())
            .with_static_gas(Gas(5 * TGAS))
            .with_attached_deposit(DEPOSIT_ONE_PROXY_TOKEN_MINT * amount.0)
            .mt_mint(
                from,
                amount,
            );

        treasury_promise.and(proxy_token_mint_promise).then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(Gas(5 * TGAS))
                    .on_action()
            )
    }

    #[private]
    pub fn on_action(&mut self) -> bool {
        require!(env::promise_results_count() == 2, "Contract expected a result on the callback");

        if is_promise_ok(env::promise_result(0)) && is_promise_ok(env::promise_result(1)) {
            true
        } else {
            env::panic_str(ERR016_ACTION_FAILED);
        }
    }

    /// sell proxy token
    pub fn sell(&mut self, token_ids: Vec<TokenId>) -> Promise {
        self.assert_not_paused();
        self.assert_is_ongoing();

        let cal_coin_amount = self.calculate_sell_proxy_token(token_ids.clone());
        assert!(cal_coin_amount > 0, "{}", ERR09_INVALID_ACTION);

        self.total_fund_amount = self.total_fund_amount.checked_sub(cal_coin_amount).unwrap();

        // Transfer stable coin to customer
        ext_fungible_token::ext(self.stable_coin_id.clone())
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .with_attached_deposit(ONE_YOCTO)
            .ft_transfer(
                env::predecessor_account_id(),
                U128::from(cal_coin_amount),
                None,
            )
            .and(
                // Burn Proxy Token
                ext_proxy_token::ext(self.proxy_token_id.clone().unwrap())
                    .with_static_gas(Gas(5 * TGAS))
                    .mt_burn(
                        env::predecessor_account_id(),
                        token_ids,
                    )
            )
            .then(
                ext_self::ext(env::current_account_id())
                .with_static_gas(Gas(5 * TGAS))
                .on_action()
            )
    }

    /// convert proxy token to real token
    pub fn convert(&mut self, token_ids: Vec<TokenId>) -> Promise {
        self.assert_not_paused();
        self.assert_is_after_buffer_period();

        let convert_project_token = match self.is_closed {
            false => {
                self.internal_project_token_mint(env::predecessor_account_id(), U128::from(token_ids.len() as u128))
            }
            true => {
                // owner
                self.internal_convert_transfer(env::predecessor_account_id(), token_ids.len() as u128)
            }
        };

        convert_project_token
            .and(
                ext_proxy_token::ext(self.proxy_token_id.clone().unwrap())
                    .with_static_gas(Gas(5 * TGAS))
                    .mt_burn(
                        env::predecessor_account_id(),
                        token_ids.clone(),
                    )
            )
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(Gas(5 * TGAS))
                    .on_convert(token_ids.len() as Balance)
            )
    }

    #[private]
    pub fn on_convert(&mut self, converted_amount: Balance) {
        require!(env::promise_results_count() == 2, "Contract expected a result on the callback");

        if is_promise_ok(env::promise_result(0)) && is_promise_ok(env::promise_result(1)) {
            self.converted_proxy_token_amount = self.converted_proxy_token_amount.checked_add(converted_amount).unwrap();
        } else {
            env::panic_str(ERR014_CONVERT_FAILED);
        }
    }

    /// claim fund
    pub fn claim_fund(&mut self, to: AccountId, amount: U128) -> Promise {
        self.assert_owner();
        self.assert_is_after_conversion_period();

        assert!(amount.0 > 0 && self.total_fund_amount >= amount.0, "{}", ERR010_INVALID_AMOUNT);

        let finder_fee_amount = amount.0.checked_mul(self.finder_fee as u128).unwrap().checked_div(FEE_DIVISOR as u128).unwrap();

        let fund_transfer = ext_fungible_token::ext(self.stable_coin_id.clone())
            .with_static_gas(Gas(5 * TGAS))
            .with_attached_deposit(ONE_YOCTO)
            .ft_transfer(
                to,
                U128::from(amount.0 - finder_fee_amount),
                None,
            );

        let finder_fee_transfer = ext_fungible_token::ext(self.stable_coin_id.clone())
            .with_static_gas(Gas(5 * TGAS))
            .with_attached_deposit(ONE_YOCTO)
            .ft_transfer(
                self.finder_id.clone().unwrap(),
                U128::from(finder_fee_amount),
                None,
            );

        fund_transfer
            .and(finder_fee_transfer)
            .then(
                ext_self::ext(env::current_account_id())
                        .with_static_gas(Gas(5 * TGAS))
                        .on_claim_fund(amount)
            )
    }

    #[private]
    pub fn on_claim_fund(&mut self, claimed_amount: U128) {
        require!(env::promise_results_count() == 2, "Contract expected a result on the callback");

        if is_promise_ok(env::promise_result(0)) && is_promise_ok(env::promise_result(1)) {
            self.total_fund_amount -= claimed_amount.0;
        } else {
            env::panic_str(ERR017_CLAIM_FUND_FAILED);
        }
    }

    /// close project
    pub fn close_project(&mut self) -> Promise {
        self.assert_owner();
        assert!(
            self.start_timestamp == 0 ||
                (self.tp_timestamp > 0 &&
                    env::block_timestamp() > self.tp_timestamp.checked_add(self.buffer_period).unwrap().checked_add(self.conversion_period).unwrap()),
            "{}",
            ERR011_NOT_AVAILABLE_TO_CLOSE
        );

        assert_eq!(self.is_closed, false, "{}", ERR013_ALREADY_CLOSED);

        let transfer_owner = match self.project_token_type {
            ProjectTokenType::Fungible => ext_fungible_token::ext(self.project_token_id.clone().unwrap())
                .with_static_gas(Gas(5 * TGAS))
                .set_owner(self.owner_id.clone()),
            ProjectTokenType::NonFungible => ext_nft_collection::ext(self.project_token_id.clone().unwrap())
                .with_static_gas(Gas(5 * TGAS))
                .set_owner(self.owner_id.clone())
        };

        if self.pre_mint_amount > 0 {
            let pre_mint = self.internal_project_token_mint(self.owner_id.clone(), U128::from(self.pre_mint_amount));
            let token_ids = (0..self.pre_mint_amount - 1).enumerate().map(|(_, token_id)| { token_id.to_string() }).collect();
            let burn_batch = ext_proxy_token::ext(self.proxy_token_id.clone().unwrap())
                .with_static_gas(Gas(5 * TGAS))
                .mt_burn(
                    self.owner_id.clone(),
                    token_ids,
                );
            burn_batch
                .and(pre_mint)
                .and(transfer_owner)
                .then(  
                    ext_self::ext(env::current_account_id())
                        .with_static_gas(Gas(5 * TGAS))
                        .on_close_project_three()
                )
        } else {
            transfer_owner
                .then(  
                    ext_self::ext(env::current_account_id())
                        .with_static_gas(Gas(5 * TGAS))
                        .on_close_project_one()
                )
        }
    }

    #[private]
    pub fn on_close_project_one(&mut self) {
        if is_promise_success() {
            env::panic_str(ERR012_CLOSE_PROJECT_FAILED);
        }
        self.is_closed = true;
    }

    pub fn on_close_project_three(&mut self) {
        require!(env::promise_results_count() == 3, "Contract expected a result on the callback");

        if is_promise_ok(env::promise_result(0)) && is_promise_ok(env::promise_result(1)) && is_promise_ok(env::promise_result(2)) {
            self.is_closed = true;
        } else {
            env::panic_str(ERR012_CLOSE_PROJECT_FAILED);
        }
    }

    pub fn internal_project_token_mint(&mut self, to: AccountId, amount: U128) -> Promise {
        match self.project_token_type {
            ProjectTokenType::NonFungible => ext_nft_collection::ext(self.project_token_id.clone().unwrap())
                .with_static_gas(Gas(5 * TGAS))
                .with_attached_deposit(DEPOSIT_ONE_NFT_MINT * amount.0)
                .nft_mint(
                    to,
                    amount,
                ),
            ProjectTokenType::Fungible => ext_fungible_token::ext(self.project_token_id.clone().unwrap())
                .with_static_gas(Gas(5 * TGAS))
                .ft_mint(
                    to,
                    amount,
                ),
        }
    }

    pub fn internal_convert_transfer(&mut self, to: AccountId, amount: u128) -> Promise {
        match self.project_token_type {
            ProjectTokenType::NonFungible => {
                let mut promise = ext_nft_collection::ext(self.project_token_id.clone().unwrap())
                    .with_static_gas(Gas(5 * TGAS))
                    .with_attached_deposit(ONE_YOCTO)
                    .nft_transfer(
                        to.clone(),
                        (self.pre_mint_amount + self.converted_proxy_token_amount).to_string(),
                        None,
                        None,
                    );
                let mut id = 1;
                while id < amount {
                    let token_id: TokenId = (self.pre_mint_amount + self.converted_proxy_token_amount + id).to_string();
                    promise = promise.and(ext_nft_collection::ext(self.project_token_id.clone().unwrap())
                        .with_static_gas(Gas(5 * TGAS))
                        .with_attached_deposit(ONE_YOCTO)
                        .nft_transfer(
                            to.clone(),
                            token_id,
                            None,
                            None,
                        ));
                    id += 1;
                }
                promise
            }
            ProjectTokenType::Fungible => ext_fungible_token::ext(self.project_token_id.clone().unwrap())
                .with_static_gas(Gas(5 * TGAS))
                .with_attached_deposit(ONE_YOCTO)
                .ft_transfer(
                    to,
                    U128::from(amount),
                    None,
                ),
        }
    }
}


#[allow(dead_code, unused)]
#[cfg(test)]
mod tests {
    use near_sdk::{test_utils::*, testing_env, AccountId, ONE_NEAR};
    use super::*;

    fn contract_account() -> AccountId {
        "contract".parse::<AccountId>().unwrap()
    }

    fn get_context(predecessor_account_id: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(contract_account())
            .account_balance(15 * ONE_NEAR)
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    #[test]
    fn test() {}
}