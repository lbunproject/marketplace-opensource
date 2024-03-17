use std::str::FromStr;
use cosmwasm_std::{
    entry_point,
    from_binary,
    to_json_binary,
    Binary,
    Deps,
    DepsMut,
    Env,
    MessageInfo,
    Response,
    StdError,
    StdResult,
    Uint128,
    Decimal,
    CosmosMsg,
    Event,
    WasmMsg,
};
use cw721::Cw721ReceiveMsg;
use cw20::{ Balance, Cw20Coin, Cw20CoinVerified, Cw20ExecuteMsg, Cw20ReceiveMsg };
use marketplace::auction::{
    Cw721HookMsg,
    ExecuteMsg,
    InstantiateMsg,
    MigrateMsg,
    QueryMsg,
    ReceiveMsg,
    DenomMsg,
    Cw20PlaceBidMsg,
};

use crate::auction::{
    admin_cancel_auction,
    admin_change_config,
    admin_pause,
    admin_resume,
    cancel_auction,
    create_auction,
    place_bid,
    set_royalty_admin,
    set_royalty_fee,
    settle_auction,
    settle_hook,
};
use crate::error::ContractError;
use crate::querier::{
    construct_action_response,
    query_all_royalty,
    query_auction,
    query_auction_by_amount,
    query_auction_by_bidder,
    query_auction_by_end_time,
    query_auction_by_nft,
    query_auction_by_seller,
    query_bid_history_by_auction_id,
    query_bid_number,
    query_calculate_price,
    query_config,
    query_nft_auction_map,
    query_not_started_auctions,
    query_royalty_admin,
    query_royalty_fee,
    query_state,
};
use crate::state::{ Config, State, CONFIG, STATE };

// Define a struct to represent token-wallet pairs
struct TokenWalletPair {
    token: &'static str,
    wallet: &'static str,
}

//FROG & BASE Tokens along with their wallet addresses and required fees
const FEE_TOKENS: &[TokenWalletPair] = &[
    TokenWalletPair {
        //FROG token
        token: "terra1wez9puj43v4s25vrex7cv3ut3w75w4h6j5e537sujyuxj0r5ne2qp9uwl9",
        wallet: "terra1409plq5jh4xn2l0xya9vav8w7ulctesmws6e7v",
    },
    TokenWalletPair {
        // BASE token
        token: "terra1uewxz67jhhhs2tj97pfm2egtk7zqxuhenm4y4m",
        wallet: "terra16kfnaknle3q0m7jd2ldvzezkd5vle3n5waf2ps", //BASE Burn wallet
    },
];

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg
) -> Result<Response, ContractError> {
    let config = Config {
        owner: info.sender,
        protocol_fee: msg.protocol_fee,
        min_reserve_price: msg.min_reserve_price,
        max_royalty_fee: msg.max_royalty_fee,
        duration: msg.duration,
        extension_duration: msg.extension_duration,
        min_increment: msg.min_increment,
        accepted_denom: msg.accepted_denom,
        collector_address: deps.api.addr_validate(&msg.collector_address)?,
    };
    if msg.max_royalty_fee + msg.protocol_fee >= Decimal::from_str("1").unwrap() {
        return Err(ContractError::InvalidRoyaltyFee {});
    }
    CONFIG.save(deps.storage, &config)?;

    let state = State {
        next_auction_id: Uint128::from(1u128),
        is_freeze: false,
    };

    STATE.save(deps.storage, &state)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ReceiveNft(msg) => receive_nft(deps, env, info, msg),
        ExecuteMsg::Receive(msg) => execute_receive(deps, env, info, msg),
        ExecuteMsg::PlaceBid { auction_id, cw_balance } => {
            match cw_balance {
                Some(cw20::Balance::Native(_)) => todo!(),
                Some(Balance::Cw20(cw20_coin)) => {
                    // cw_balance is Some(Cw20), extract cw20_coin and call place_bid
                    place_bid(deps, env, info, auction_id, Some(cw20::Balance::Cw20(cw20_coin)))
                },
                None => {
                    // cw_balance is None, call place_bid without a balance
                    place_bid(deps, env, info, auction_id, None)
                }
            }
        },
        ExecuteMsg::Settle { auction_id } => settle_auction(deps, env, auction_id),
        ExecuteMsg::CancelAuction { auction_id } => cancel_auction(deps, env, info, auction_id),
        ExecuteMsg::AdminCancelAuction { auction_id } => {
            admin_cancel_auction(deps, env, info, auction_id)
        }
        ExecuteMsg::AdminPause {} => admin_pause(deps, env, info),
        ExecuteMsg::AdminResume {} => admin_resume(deps, env, info),
        ExecuteMsg::AdminChangeConfig {
            protocol_fee,
            min_increment,
            min_reserve_price,
            max_royalty_fee,
            duration,
            extension_duration,
            accepted_denom,
            collector_address,
        } =>
            admin_change_config(
                deps,
                env,
                info,
                protocol_fee,
                min_increment,
                min_reserve_price,
                max_royalty_fee,
                duration,
                extension_duration,
                accepted_denom,
                collector_address
            ),
        ExecuteMsg::SetRoyaltyFee { contract_addr, royalty_fee, creator } =>
            set_royalty_fee(deps, env, info, contract_addr, creator, royalty_fee),
        ExecuteMsg::SetRoyaltyAdmin { address, enable } => {
            set_royalty_admin(deps, env, info, address, enable)
        }
        ExecuteMsg::SettleHook { nft_contract, token_id, owner } =>
            settle_hook(deps, env, info, nft_contract, token_id, owner),
    }
}

pub fn receive_nft(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw721_msg: Cw721ReceiveMsg
) -> Result<Response, ContractError> {
    match from_binary(&cw721_msg.msg) {
        Ok(Cw721HookMsg::CreateAuction { denom, reserve_price, is_instant_sale }) => {
            // need to check that this contract is owner of nft to prevent malicious contract call this function directly

            let seller = deps.api.addr_validate(&cw721_msg.sender)?;
            let nft_contract = info.sender;
            let token_id = cw721_msg.token_id.clone();
            create_auction(
                deps,
                env,
                nft_contract,
                token_id,
                seller,
                denom,
                reserve_price,
                is_instant_sale
            )
        }
        Err(err) => Err(ContractError::Std(StdError::generic_err(err.to_string()))),
    }
}

pub fn execute_receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg
) -> Result<Response, ContractError> {
    let msg: ReceiveMsg = from_binary(&wrapper.msg)?;

    let cw_balance = Balance::Cw20(Cw20CoinVerified {
        address: info.sender.clone(), //info.sender is the CW20 Token contract
        amount: wrapper.amount,
    });

    //Make new info with the sender set to who actually sent the cw20 tokens
    let api = deps.api;
    let wrapper_sender = &api.addr_validate(&wrapper.sender)?;
    let new_info = MessageInfo {
        sender: wrapper_sender.clone(),
        funds: vec![],
    };

    match msg {
        ReceiveMsg::PayFee(msg) =>
            pay_fee(deps, env, info, msg, wrapper_sender.to_string(), cw_balance),
        ReceiveMsg::PlaceBidCw20(msg) => {
            let auction_id = msg.auction_id;
            let result = place_bid(deps, env, new_info, auction_id, Some(cw_balance))?;
            // Handle the result appropriately
            Ok(result)}
        _ => {
            return Err(ContractError::TracePoint {
                location: "Unknown Message".to_string(),
            });
        }
    }
}

pub fn pay_fee(
    mut deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: DenomMsg,
    wrapper_sender: String, // Who actually sent the deposit
    cw_balance: Balance
) -> Result<Response, ContractError> {
    //Balance can not be empty
    if cw_balance.is_empty() {
        return Err(ContractError::EmptyBalance {});
    }

    // Determine the denom of total_funds_in
    let denom = if let Balance::Cw20(token) = &cw_balance {
        token.address.to_string()
    } else {
        unreachable!(); // This branch should never be reached
    };

    let (token_used, wallet_paired);

    // Check if denom matches any token
    if let Some(pair) = FEE_TOKENS.iter().find(|pair| pair.token == denom) {
        // Save the token address used
        token_used = pair.token;
        // You may also want to store the associated wallet address
        wallet_paired = pair.wallet;
    } else {
        // Return an error if denom does not match any token
        return Err(ContractError::InvalidToken {});
    }

    // Determine the total_funds_in amount
    let amount = if let Balance::Cw20(token) = &cw_balance {
        token.amount
    } else {
        unreachable!(); // This branch should never be reached
    };

    // Transfer the CW20 tokens to the owner
    let fee_collector_addr = wallet_paired.to_string();
    let cw20_token_addr = token_used.to_string();
    let fee_amount = amount; // Amount to transfer

    // Create a CW20 transfer message
    let cw20_transfer_msg = WasmMsg::Execute {
        contract_addr: cw20_token_addr.clone(),
        msg: to_json_binary(
            &(Cw20ExecuteMsg::Transfer {
                recipient: fee_collector_addr.to_string(),
                amount: fee_amount,
            })
        )?,
        funds: vec![],
    };

    Ok(
        Response::new()
            .add_event(
                Event::new("fee_paid")
                    .add_attribute("denom", token_used.to_string())
                    .add_attribute("amount", fee_amount.to_string())
                    .add_attribute("fee_collector", fee_collector_addr.to_string())
            )
            .add_message(CosmosMsg::Wasm(cw20_transfer_msg))
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::State {} => to_json_binary(&query_state(deps)?),
        QueryMsg::Auction { auction_id } => to_json_binary(&query_auction(deps, auction_id)?),
        QueryMsg::RoyaltyFee { contract_addr } => {
            to_json_binary(&query_royalty_fee(deps, contract_addr)?)
        }
        QueryMsg::RoyaltyAdmin { address } => to_json_binary(&query_royalty_admin(deps, address)?),
        QueryMsg::AllRoyaltyFee { start_after, limit } => {
            to_json_binary(&query_all_royalty(deps, start_after, limit)?)
        }
        QueryMsg::CalculatePrice { nft_contract, token_id, amount } =>
            to_json_binary(&query_calculate_price(deps, nft_contract, token_id, amount)?),
        QueryMsg::NftAuction { nft_contract, token_id } =>
            to_json_binary(&query_nft_auction_map(deps, nft_contract, token_id)?),
        QueryMsg::BidHistoryByAuctionId { auction_id, limit } => {
            to_json_binary(&query_bid_history_by_auction_id(deps, auction_id, limit)?)
        }
        QueryMsg::BidsCount { auction_id } => to_json_binary(&query_bid_number(deps, auction_id)?),
        QueryMsg::AuctionByContract { nft_contract, limit } => {
            let auction_ids = query_auction_by_nft(deps, nft_contract, limit)?;
            to_json_binary(&construct_action_response(deps, auction_ids)?)
        }
        QueryMsg::AuctionBySeller { seller, limit } => {
            let auction_ids = query_auction_by_seller(deps, seller, limit)?;
            to_json_binary(&construct_action_response(deps, auction_ids)?)
        }
        QueryMsg::AuctionByEndTime { nft_contract, end_time, limit, is_desc } => {
            let auction_ids = query_auction_by_end_time(
                deps,
                nft_contract,
                end_time,
                limit,
                is_desc
            )?;
            to_json_binary(&construct_action_response(deps, auction_ids)?)
        }
        QueryMsg::AuctionByAmount { nft_contract, amount, limit } => {
            let auction_ids = query_auction_by_amount(deps, nft_contract, amount, limit)?;
            to_json_binary(&construct_action_response(deps, auction_ids)?)
        }
        QueryMsg::NotStartedAuction { nft_contract, start_after, limit, is_desc } => {
            let auction_ids = query_not_started_auctions(
                deps,
                nft_contract,
                start_after,
                limit,
                is_desc
            )?;
            to_json_binary(&construct_action_response(deps, auction_ids)?)
        }
        QueryMsg::AuctionByBidder { bidder, start_after, limit } => {
            let auction_ids = query_auction_by_bidder(deps, bidder, start_after, limit)?;
            to_json_binary(&construct_action_response(deps, auction_ids)?)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
