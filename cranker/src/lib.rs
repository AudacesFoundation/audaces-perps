use audaces_protocol::{
    instruction::{
        collect_garbage, crank_funding, crank_liquidation, extract_funding, InstanceContext,
        MarketContext,
    },
    processor::FIDA_BNB,
    state::{
        instance::Instance, instance::PageInfo, market::MarketState, user_account::OpenPosition,
        user_account::UserAccountState, StateObject,
    },
};
use error::CrankError;
use futures::{
    stream::{self, Iter},
    StreamExt,
};
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::RpcAccountInfoConfig,
    rpc_config::{RpcProgramAccountsConfig, RpcSendTransactionConfig},
    rpc_filter::{self, Memcmp, RpcFilterType},
};
use solana_program::{program_pack::Pack, pubkey::Pubkey};
use solana_sdk::{
    account::Account,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use spl_associated_token_account::get_associated_token_address;
use std::{
    borrow::Borrow,
    str::FromStr,
    sync::Arc,
    time::{Duration, SystemTime},
    vec::IntoIter,
};
use tokio::{
    runtime::Runtime,
    sync::Mutex,
    task::{self, JoinError},
    time::interval,
};

use crate::utils::no_op_filter;

pub mod error;

mod utils;

pub struct Context {
    pub program_id: Pubkey,
    pub market: Pubkey,
    pub fee_payer: Keypair,
    pub endpoint: String,
    pub num_threads: usize,
}

const LIQUIDATION_PERIOD: u64 = 1_000;
const FUNDING_PERIOD: u64 = 1_000;
const FUNDING_EXTRACTION_PERIOD: u64 = 1_800_000;
const GARBAGE_COLLECTION_PERIOD: u64 = 10_000;
const GARBAGE_COLLECT_MAX_ITERATIONS: u64 = 500;

impl Context {
    pub fn crank_liquidation(self) {
        let connection = RpcClient::new(self.endpoint.clone());
        let (market_ctx, quote_mint) =
            get_market(self.program_id, self.market, &connection).unwrap();
        println!("Market quote mint {:?}", quote_mint);

        let endpoint = Arc::new(self.endpoint.clone());
        let market = Arc::new(market_ctx);

        let target_token_account = Arc::new(get_associated_token_address(
            &self.fee_payer.pubkey(),
            &quote_mint,
        ));
        let fee_payer = Arc::new(self.fee_payer);

        let rt = Runtime::new().unwrap();

        let mut tasks = Vec::with_capacity(market.instances.len());

        println!("Found {} instances", market.instances.len());

        for i in 0..market.instances.len() {
            let t = run_liquidation(
                Arc::clone(&endpoint),
                Arc::clone(&market),
                i,
                Arc::clone(&target_token_account),
                Arc::clone(&fee_payer),
            );
            tasks.push(t);
        }

        for t in tasks {
            rt.block_on(t).unwrap();
        }
    }

    pub fn crank_funding(self) {
        let connection = RpcClient::new(self.endpoint.clone());
        let (market_ctx, _) = get_market(self.program_id, self.market, &connection).unwrap();
        let market = Arc::new(market_ctx);
        let fee_payer = Arc::new(self.fee_payer);

        let rt = Runtime::new().unwrap();
        let _guard = rt.enter();

        let instruction = crank_funding(&market);
        let t = task::spawn(async move {
            let mut ticker = interval(Duration::from_millis(FUNDING_PERIOD));
            loop {
                ticker.tick().await;
                let transaction =
                    Transaction::new_with_payer(&[instruction.clone()], Some(&fee_payer.pubkey()));
                let sig = utils::retry(
                    transaction,
                    |t| {
                        let mut tr = t.clone();
                        let (recent_blockhash, _) = connection.get_recent_blockhash()?;
                        tr.partial_sign::<Vec<&Keypair>>(
                            &vec![fee_payer.borrow()],
                            recent_blockhash,
                        );
                        connection.send_and_confirm_transaction(&tr)
                    },
                    no_op_filter,
                )
                .await;
                println!("Sent funding transaction {:?}", sig);
            }
        });

        rt.block_on(t).unwrap();
    }
    pub fn crank_funding_extraction(self, swarm_size: u16, node_id: u8) {
        let s = Arc::new(self);
        let rt = Runtime::new().unwrap();
        let _guard = rt.enter();
        let t = async move {
            let mut ticker = interval(Duration::from_millis(FUNDING_EXTRACTION_PERIOD));
            loop {
                ticker.tick().await;
                let start_time = SystemTime::now();
                crank_funding_extraction_iteration(&s, swarm_size, node_id).await;
                let end_time = SystemTime::now();
                println!(
                    "Finished funding extraction cycle in {:?}s within a funding period of {:?}s",
                    end_time.duration_since(start_time).unwrap().as_secs_f64(),
                    FUNDING_PERIOD / 1000
                )
            }
        };
        rt.block_on(t);
    }

    pub fn garbage_collect(self) {
        let s = Arc::new(self);
        let rt = Runtime::new().unwrap();
        let _guard = rt.enter();
        let connection = RpcClient::new(String::clone(&s.endpoint));
        let (market, quote_mint) = get_market(s.program_id, s.market, &connection).unwrap();
        let target_token_account = Arc::new(get_associated_token_address(
            &s.fee_payer.pubkey(),
            &quote_mint,
        ));
        let market = Arc::new(market);
        let t = task::spawn(async move {
            let mut ticker = interval(Duration::from_millis(GARBAGE_COLLECTION_PERIOD));
            loop {
                ticker.tick().await;
                crank_garbage_collection(&s, &market, &target_token_account).await;
            }
        });
        rt.block_on(t).unwrap();
    }
}

pub fn get_market(
    program_id: Pubkey,
    market_key: Pubkey,
    connection: &RpcClient,
) -> Result<(MarketContext, Pubkey), CrankError> {
    let market_data = connection
        .get_account_data(&market_key)
        .map_err(|_| CrankError::ConnectionError)?;
    let market_state =
        MarketState::unpack_from_slice(&market_data).map_err(|_| CrankError::InvalidMarketState)?;
    let instance_addresses = market_data
        [MarketState::LEN..MarketState::LEN + (market_state.number_of_instances as usize) * 32]
        .chunks(32)
        .into_iter()
        .map(|s| Pubkey::new(s));

    let instances = instance_addresses
        .map(|a| {
            let instance_data = connection
                .get_account_data(&a)
                .map_err(|_| CrankError::ConnectionError)?;
            let instance = Instance::unpack_from_slice(&instance_data)
                .map_err(|_| CrankError::InvalidMarketState)?;
            let memory_pages = instance_data[Instance::LEN..]
                .chunks(PageInfo::LEN)
                .map(|s| {
                    Ok(Pubkey::new(
                        &PageInfo::unpack_from_slice(s)
                            .map_err(|_| CrankError::InvalidMarketState)?
                            .address,
                    ))
                })
                .take(instance.number_of_pages as usize)
                .collect::<Result<Vec<_>, _>>()?;

            Ok(InstanceContext {
                instance_account: a,
                memory_pages,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let market_signer_account = Pubkey::create_program_address(
        &[&market_key.to_bytes(), &[market_state.signer_nonce]],
        &program_id,
    )
    .map_err(|_| CrankError::InvalidMarketState)?;

    let token_account = connection
        .get_token_account(&Pubkey::new(&market_state.vault_address))
        .map_err(|_| CrankError::ConnectionError)?
        .unwrap();

    let ctx = MarketContext {
        audaces_protocol_program_id: program_id,
        signer_nonce: market_state.signer_nonce,
        market_signer_account,
        oracle_account: Pubkey::new(&market_state.oracle_address),
        market_account: market_key,
        admin_account: Pubkey::new(&market_state.admin_address),
        market_vault: Pubkey::new(&market_state.vault_address),
        bonfida_bnb: Pubkey::from_str(FIDA_BNB).unwrap(),
        instances,
    };

    Ok((ctx, Pubkey::from_str(&token_account.mint).unwrap()))
}

async fn run_liquidation(
    endpoint: Arc<String>,
    market: Arc<MarketContext>,
    instance_index: usize,
    target_token_account: Arc<Pubkey>,
    fee_payer: Arc<Keypair>,
) -> Result<(), JoinError> {
    task::spawn(async move {
        let connection = RpcClient::new(String::clone(&endpoint));
        let liquidation_instruction = crank_liquidation(
            &market,
            instance_index as u8,
            *target_token_account.borrow(),
        );
        println!("Starting liquidation task");
        let mut ticker = interval(Duration::from_millis(LIQUIDATION_PERIOD));
        loop {
            ticker.tick().await;
            println!("Liquidation tick");
            let transaction = Transaction::new_with_payer(
                &[liquidation_instruction.clone()],
                Some(&fee_payer.pubkey()),
            );
            let sig = utils::retry(
                transaction,
                |t| {
                    let (recent_blockhash, _) = connection.get_recent_blockhash()?;
                    let mut tr = t.clone();
                    tr.partial_sign::<Vec<&Keypair>>(&vec![fee_payer.borrow()], recent_blockhash);
                    connection.send_transaction_with_config(
                        &tr,
                        RpcSendTransactionConfig {
                            skip_preflight: false,
                            preflight_commitment: None,
                            ..RpcSendTransactionConfig::default()
                        },
                    )
                },
                no_op_filter,
            )
            .await;
            println!(
                "Sent liquidation transaction for instance {:?} with signature {:?}",
                instance_index, sig
            );
        }
    })
    .await
}

async fn crank_garbage_collection(
    ctx: &Arc<Context>,
    market: &Arc<MarketContext>,
    target_token_account: &Arc<Pubkey>,
) {
    let connection = RpcClient::new(String::clone(&ctx.endpoint));
    for i in 0..(market.instances.len() as u8) {
        let instruction = collect_garbage(
            &market,
            i,
            GARBAGE_COLLECT_MAX_ITERATIONS,
            **target_token_account,
        );
        let transaction =
            Transaction::new_with_payer(&[instruction], Some(&ctx.fee_payer.pubkey()));
        let sig = utils::retry(
            transaction,
            |t| {
                let mut tr = t.clone();
                let (recent_blockhash, _) = connection.get_recent_blockhash()?;
                tr.partial_sign(&[&ctx.fee_payer], recent_blockhash);
                connection.send_transaction_with_config(
                    &tr,
                    RpcSendTransactionConfig {
                        skip_preflight: false,
                        preflight_commitment: None,
                        ..RpcSendTransactionConfig::default()
                    },
                )
            },
            no_op_filter,
        )
        .await;
        println!(
            "Sent garbage collection transaction for isntance {:?} with signature {:?}",
            i, sig
        );
    }
}
async fn crank_funding_extraction_iteration(ctx: &Arc<Context>, swarm_size: u16, node_id: u8) {
    if swarm_size == 0 {
        panic!("Swarm size should be non-zero");
    }
    if !swarm_size.is_power_of_two() {
        panic!("Swarm size must be a power of two");
    }
    if swarm_size > 256 {
        panic!("Maximum supported swarm size is 256");
    }
    if node_id as u16 >= swarm_size {
        panic!("Node id should be less than swarm size.")
    }

    let res = (0..(256 / (swarm_size as u16))).map(move |id| ((id * swarm_size) as u8) + node_id);
    let configs = if swarm_size > 1 {
        res.map(|m| RpcProgramAccountsConfig {
            filters: Some(vec![
                // Filter for user accounts
                RpcFilterType::Memcmp(Memcmp {
                    offset: 0,
                    bytes: rpc_filter::MemcmpEncodedBytes::Binary(
                        bs58::encode(vec![StateObject::UserAccount as u8]).into_string(),
                    ),
                    encoding: None,
                }),
                // Filter for a subset of owners
                RpcFilterType::Memcmp(Memcmp {
                    offset: 2,
                    bytes: rpc_filter::MemcmpEncodedBytes::Binary(
                        bs58::encode(vec![m]).into_string(),
                    ),
                    encoding: None,
                }),
                // Filter for active user accounts (with open positions)
                RpcFilterType::Memcmp(Memcmp {
                    offset: 34,
                    bytes: rpc_filter::MemcmpEncodedBytes::Binary(
                        bs58::encode(vec![1]).into_string(),
                    ),
                    encoding: None,
                }),
                // Filter for user accounts affiliated with the current market
                RpcFilterType::Memcmp(Memcmp {
                    offset: 35,
                    bytes: rpc_filter::MemcmpEncodedBytes::Binary(ctx.market.to_string()),
                    encoding: None,
                }),
            ]),
            account_config: RpcAccountInfoConfig {
                encoding: None,
                data_slice: None,
                commitment: None,
            },
            with_context: None,
        })
        .collect::<Vec<_>>()
    } else {
        vec![RpcProgramAccountsConfig {
            filters: Some(vec![
                // Filter for user accounts
                RpcFilterType::Memcmp(Memcmp {
                    offset: 0,
                    bytes: rpc_filter::MemcmpEncodedBytes::Binary(
                        bs58::encode(&[StateObject::UserAccount as u8]).into_string(),
                    ),
                    encoding: None,
                }),
                // Filter for active user accounts (with open positions)
                RpcFilterType::Memcmp(Memcmp {
                    offset: 34,
                    bytes: rpc_filter::MemcmpEncodedBytes::Binary(bs58::encode(&[1]).into_string()),
                    encoding: None,
                }),
                // Filter for user accounts affiliated with the current market
                RpcFilterType::Memcmp(Memcmp {
                    offset: 35,
                    bytes: rpc_filter::MemcmpEncodedBytes::Binary(ctx.market.to_string()),
                    encoding: None,
                }),
            ]),
            account_config: RpcAccountInfoConfig {
                encoding: None,
                data_slice: None,
                commitment: None,
            },
            with_context: None,
        }]
    };
    let url = ctx.endpoint.clone();
    let program_id = ctx.program_id;
    let accounts = stream::iter(configs.into_iter())
        .then(move |c| account_stream(program_id, url.clone(), c))
        .flatten();
    let connection = RpcClient::new(ctx.endpoint.to_owned());

    let accounts_mutex = Arc::new(Mutex::new(Box::pin(accounts)));
    let (market, _) = utils::retry(
        &connection,
        |c| get_market(ctx.program_id, ctx.market, &c),
        |r| r,
    )
    .await;
    let market = Arc::new(market);
    let mut tasks = Vec::with_capacity(num_cpus::get());
    for _ in 0..tasks.capacity() {
        let task_mutex = Arc::clone(&accounts_mutex);
        let connection = RpcClient::new(ctx.endpoint.to_owned());
        let c = Arc::clone(&ctx);
        let m = Arc::clone(&market);
        let t = async move {
            loop {
                // Can't use if let here due to borrow checker in an async context
                let next = {
                    let mut f = task_mutex.lock().await;
                    f.next().await
                };
                if next.is_none() {
                    break;
                };
                let (k, a): (Pubkey, Account) = next.unwrap();
                println!("Processing funding for {:?}", k);
                let fee_payer_pk = c.fee_payer.pubkey();
                let transactions = {
                    let mut position_offset = UserAccountState::LEN;
                    let header =
                        UserAccountState::unpack_from_slice(&a.data[..UserAccountState::LEN])
                            .unwrap();
                    let mut cranked_instance_indices: Vec<u8> = vec![0; m.instances.len()];
                    let mut instructions = vec![];
                    for _ in 0..header.number_of_open_positions {
                        let position = OpenPosition::unpack_from_slice(
                            &a.data[position_offset..position_offset + OpenPosition::LEN],
                        )
                        .unwrap();
                        cranked_instance_indices[position.instance_index as usize] = 1;
                        instructions.push(extract_funding(&m, position.instance_index, k));
                        position_offset += OpenPosition::LEN;
                    }
                    for (i, l) in cranked_instance_indices.iter().enumerate() {
                        if *l == 0 {
                            continue;
                        }
                        instructions.push(extract_funding(&m, i as u8, k))
                    }
                    instructions
                        .into_iter()
                        .map(|i| Transaction::new_with_payer(&[i], Some(&fee_payer_pk)))
                };
                for t in transactions {
                    let sig = utils::retry(
                        t,
                        |t| {
                            let mut tr = t.clone();
                            let (recent_blockhash, _) = connection.get_recent_blockhash()?;
                            tr.partial_sign::<Vec<&Keypair>>(&vec![&c.fee_payer], recent_blockhash);
                            connection.send_transaction_with_config(
                                &tr,
                                RpcSendTransactionConfig {
                                    skip_preflight: false,
                                    ..RpcSendTransactionConfig::default()
                                },
                            )
                        },
                        no_op_filter,
                    )
                    .await;
                    println!("Sent funding extraction transaction {:?}", sig);
                }
            }
        };
        tasks.push(task::spawn(t))
    }
    for t in tasks {
        t.await.unwrap();
    }
}

async fn account_stream(
    program_id: Pubkey,
    url: String,
    c: RpcProgramAccountsConfig,
) -> Iter<IntoIter<(Pubkey, Account)>> {
    let k: Vec<(Pubkey, Account)> = utils::retry(
        c,
        move |conf| {
            let conn = RpcClient::new(url.clone());
            conn.get_program_accounts_with_config(&program_id, conf.to_owned())
        },
        |r| r,
    )
    .await;
    stream::iter(k)
}
