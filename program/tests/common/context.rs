use std::{rc::Rc, str::FromStr};

use super::utils;
use audaces_protocol::{
    error::PerpError,
    instruction::MarketContext,
    positions_book::{
        memory::{Memory, SLOT_SIZE, TAG_SIZE},
        page::Page,
    },
    processor::FIDA_BNB,
    state::{
        instance::parse_instance,
        instance::Instance,
        instance::PageInfo,
        market::get_instance_address,
        market::{MarketDataPoint, MarketState},
        user_account::OpenPosition,
        user_account::UserAccountState,
    },
    utils::{get_oracle_price, get_tree_depth, print_tree},
};
use mock_oracle::instruction::change_price;
use solana_program::{
    entrypoint::ProgramResult, program_error::ProgramError, program_pack::Pack, pubkey::Pubkey,
    system_instruction::create_account,
};
use solana_program_test::{processor, ProgramTest, ProgramTestContext};
use solana_sdk::signature::Keypair;
use solana_sdk::{signature::Signer, transport::TransportError};
use spl_token::{
    instruction::mint_to,
    state::{Account, AccountState},
};
use std::cell::RefCell;
use utils::{
    create_and_get_associated_token_address, mint_init_transaction, sign_send_instructions,
};

pub struct TestContext {
    pub mock_oracle_program_id: Pubkey,
    pub usdc_mint: Keypair,
    pub usdc_mint_authority: Keypair,
    pub market_admin_keypair: Keypair,
    pub coin_decimals: u8,
    pub quote_decimals: u8,
}

pub struct UserContext {
    pub owner_account: Keypair,
    pub usdc_account: Pubkey,
    pub user_accounts: Vec<Pubkey>,
}

pub struct Context {
    pub prg_test_ctx: ProgramTestContext,
    pub test_ctx: TestContext,
    pub market_ctx: MarketContext,
    pub user_ctx: UserContext,
}

impl Context {
    pub async fn init(vault_init_amount: u64, coin_decimals: u8, quote_decimals: u8) -> Context {
        // Create program and test environment
        let audaces_protocol_program_id =
            Pubkey::from_str("AudacesXCWuBvfkegQfZyiNwAJb9Ss623VQ5DA111111").unwrap();

        let test_ctx = TestContext {
            mock_oracle_program_id: Pubkey::from_str(
                "BudacesXCWuBvfkegQfZyiNwAJb9Ss623VQ5DA111111",
            )
            .unwrap(),
            usdc_mint: Keypair::new(),
            usdc_mint_authority: Keypair::new(),
            market_admin_keypair: Keypair::new(),
            coin_decimals,
            quote_decimals,
        };

        let mut program_test = ProgramTest::new(
            "audaces_protocol",
            audaces_protocol_program_id,
            processor!(audaces_protocol::entrypoint::process_instruction),
        );
        program_test.add_program(
            "mock_oracle",
            test_ctx.mock_oracle_program_id,
            processor!(mock_oracle::entrypoint::process_instruction),
        );

        let mut data = vec![0; Account::LEN];
        Account {
            mint: test_ctx.usdc_mint.pubkey(),
            owner: test_ctx.market_admin_keypair.pubkey(),
            amount: 0,
            state: AccountState::Initialized,
            ..Account::default()
        }
        .pack_into_slice(&mut data);

        program_test.add_account(
            Pubkey::from_str(FIDA_BNB).unwrap(),
            solana_sdk::account::Account {
                lamports: 1_000_000,
                owner: spl_token::id(),
                executable: false,
                rent_epoch: 0,
                data,
            },
        );

        // Create Market context
        let mut prg_test_ctx = program_test.start_with_context().await;

        let market_account = Keypair::new();
        let oracle_account = Keypair::new();

        let user_open_position_account = Keypair::new();
        let mut user_ctx = UserContext {
            usdc_account: Pubkey::default(), // Placeholder
            owner_account: Keypair::new(),
            user_accounts: vec![user_open_position_account.pubkey()],
        };

        let (market_signer_key, market_signer_nonce) = Pubkey::find_program_address(
            &[&market_account.pubkey().to_bytes()],
            &audaces_protocol_program_id,
        );

        // Setup the accounts and tokens
        let mint_init_transaction = mint_init_transaction(
            &prg_test_ctx,
            &test_ctx.usdc_mint,
            &test_ctx.usdc_mint_authority,
        );
        prg_test_ctx
            .banks_client
            .process_transaction(mint_init_transaction)
            .await
            .unwrap();

        let space = 1_000_000;
        let create_market_account_instruction = create_account(
            &prg_test_ctx.payer.pubkey(),
            &market_account.pubkey(),
            prg_test_ctx
                .banks_client
                .get_rent()
                .await
                .unwrap()
                .minimum_balance(space),
            space as u64,
            &audaces_protocol_program_id,
        );
        sign_send_instructions(
            &mut prg_test_ctx,
            vec![create_market_account_instruction],
            vec![&market_account],
        )
        .await
        .unwrap();

        let open_position_account_instruction = create_account(
            &prg_test_ctx.payer.pubkey(),
            &user_open_position_account.pubkey(),
            prg_test_ctx
                .banks_client
                .get_rent()
                .await
                .unwrap()
                .minimum_balance(space),
            space as u64,
            &audaces_protocol_program_id,
        );
        sign_send_instructions(
            &mut prg_test_ctx,
            vec![open_position_account_instruction],
            vec![&user_open_position_account],
        )
        .await
        .unwrap();

        let oracle_account_instruction = create_account(
            &prg_test_ctx.payer.pubkey(),
            &oracle_account.pubkey(),
            1_000_000,
            8,
            &test_ctx.mock_oracle_program_id,
        );
        sign_send_instructions(
            &mut prg_test_ctx,
            vec![oracle_account_instruction],
            vec![&oracle_account],
        )
        .await
        .unwrap();

        let (create_market_vault_account_transaction, market_vault_key) =
            create_and_get_associated_token_address(
                &prg_test_ctx,
                &market_signer_key,
                &test_ctx.usdc_mint.pubkey(),
            );
        prg_test_ctx
            .banks_client
            .process_transaction(create_market_vault_account_transaction)
            .await
            .unwrap();

        let (create_source_asset_transaction, user_usdc_key) =
            create_and_get_associated_token_address(
                &prg_test_ctx,
                &user_ctx.owner_account.pubkey(),
                &test_ctx.usdc_mint.pubkey(),
            );
        user_ctx.usdc_account = user_usdc_key;
        prg_test_ctx
            .banks_client
            .process_transaction(create_source_asset_transaction)
            .await
            .unwrap();

        let market_ctx = MarketContext {
            audaces_protocol_program_id,
            signer_nonce: market_signer_nonce,
            market_signer_account: market_signer_key,
            oracle_account: oracle_account.pubkey(),
            market_account: market_account.pubkey(),
            admin_account: test_ctx.market_admin_keypair.pubkey(),
            market_vault: market_vault_key,
            bonfida_bnb: Pubkey::from_str(FIDA_BNB).unwrap(),
            instances: vec![],
        };

        // Mint tokens to vault and source
        let source_mint_instruction = mint_to(
            &spl_token::id(),
            &test_ctx.usdc_mint.pubkey(),
            &user_usdc_key,
            &test_ctx.usdc_mint_authority.pubkey(),
            &[],
            1 << 63,
        )
        .unwrap();
        let vault_mint_instruction = mint_to(
            &spl_token::id(),
            &test_ctx.usdc_mint.pubkey(),
            &market_ctx.market_vault,
            &test_ctx.usdc_mint_authority.pubkey(),
            &[],
            vault_init_amount,
        )
        .unwrap();
        sign_send_instructions(
            &mut prg_test_ctx,
            vec![source_mint_instruction],
            vec![&test_ctx.usdc_mint_authority],
        )
        .await
        .unwrap();
        sign_send_instructions(
            &mut prg_test_ctx,
            vec![vault_mint_instruction],
            vec![&test_ctx.usdc_mint_authority],
        )
        .await
        .unwrap();

        return Context {
            prg_test_ctx,
            test_ctx,
            market_ctx,
            user_ctx,
        };
    }

    pub async fn change_oracle_price(&mut self, new_price: u64) -> Result<(), TransportError> {
        let change_price_instruction = change_price(
            self.test_ctx.mock_oracle_program_id,
            new_price,
            self.market_ctx.oracle_account,
        )
        .unwrap();
        sign_send_instructions(
            &mut self.prg_test_ctx,
            vec![change_price_instruction],
            vec![],
        )
        .await
    }

    // Getter functions

    pub async fn get_position(
        &mut self,
        position_index: u16,
        user_account_index: usize,
    ) -> Result<OpenPosition, ProgramError> {
        let user_account = self
            .prg_test_ctx
            .banks_client
            .get_account(self.user_ctx.user_accounts[user_account_index])
            .await
            .unwrap()
            .unwrap();
        let offset = (position_index as usize)
            .checked_mul(OpenPosition::LEN)
            .and_then(|s| s.checked_add(UserAccountState::LEN))
            .ok_or(PerpError::Overflow)?;
        let offset_end = offset
            .checked_add(OpenPosition::LEN)
            .ok_or(PerpError::Overflow)?;
        let slice = user_account
            .data
            .get(offset..offset_end)
            .ok_or(ProgramError::InvalidArgument)?;
        OpenPosition::unpack_unchecked(slice)
    }

    pub async fn get_user_account(
        &mut self,
        user_account_index: usize,
    ) -> Result<UserAccountState, ProgramError> {
        let user_account = self
            .prg_test_ctx
            .banks_client
            .get_account(self.user_ctx.user_accounts[user_account_index])
            .await
            .unwrap()
            .unwrap();
        UserAccountState::unpack_from_slice(&user_account.data)
    }

    pub async fn get_market_state(&mut self) -> Result<MarketState, ProgramError> {
        let market_account = self
            .prg_test_ctx
            .banks_client
            .get_account(self.market_ctx.market_account)
            .await
            .unwrap()
            .unwrap();
        Ok(MarketState::unpack_from_slice(&market_account.data)?)
    }

    pub async fn get_page_datas(
        &mut self,
        page_infos: &[PageInfo],
    ) -> Result<Vec<(solana_sdk::account::Account, u32, Option<u32>)>, ProgramError> {
        let mut page_datas = Vec::with_capacity(page_infos.len());
        for p in page_infos {
            let page_data = self
                .prg_test_ctx
                .banks_client
                .get_account(Pubkey::new(&p.address))
                .await
                .unwrap()
                .unwrap();
            page_datas.push((page_data, p.unitialized_memory_index, p.free_slot_list_hd));
        }
        Ok(page_datas)
    }

    pub async fn get_market_data(&mut self) -> Result<MarketDataPoint, ProgramError> {
        let market_account = self
            .prg_test_ctx
            .banks_client
            .get_account(self.market_ctx.market_account)
            .await
            .unwrap()
            .unwrap();
        let market_state = MarketState::unpack_from_slice(&market_account.data)?;
        let market_vault_balance = self.get_market_vault_balance().await.unwrap();

        let mut instances = Vec::with_capacity(market_state.number_of_instances as usize);
        for i in 0..market_state.number_of_instances {
            let instance_address = self.get_instance_address(i).await.unwrap();
            let (instance, page_infos) = self.parse_instance(instance_address).await.unwrap();
            instances.push((instance, page_infos));
        }

        let mut gc_list_lengths = Vec::with_capacity(market_state.number_of_instances as usize);
        let mut page_full_ratios = Vec::with_capacity(market_state.number_of_instances as usize);
        let mut longs_depths = Vec::with_capacity(market_state.number_of_instances as usize);
        let mut shorts_depths = Vec::with_capacity(market_state.number_of_instances as usize);
        for (instance, page_infos) in &instances {
            let mut page_datas = self.get_page_datas(&page_infos).await?;
            let mut pages = Vec::with_capacity(page_datas.len());
            let mut instance_page_full_ratios = vec![];
            for (page_data, u_mem_index, free_slot_list_hd) in &mut page_datas {
                let page = Page {
                    page_size: ((page_data.data.len() - TAG_SIZE) / SLOT_SIZE) as u32,
                    data: Rc::new(RefCell::new(&mut page_data.data)),
                    uninitialized_memory: u_mem_index.to_owned(),
                    free_slot_list_hd: free_slot_list_hd.to_owned(),
                };
                let page_ratio = ((page.uninitialized_memory as f64)
                    - (page.get_nb_free_slots().unwrap() as f64))
                    / (page.page_size as f64);
                instance_page_full_ratios.push(page_ratio);
                pages.push(page);
            }
            page_full_ratios.push(instance_page_full_ratios);
            let mem = Memory::new(pages, instance.garbage_pointer);
            let (longs_depth, shorts_depth) = self.get_tree_depth(instance, &mem).await;
            longs_depths.push(longs_depth as u64);
            shorts_depths.push(shorts_depth as u64);
            gc_list_lengths.push(mem.get_gc_list_len().unwrap());
        }
        let insurance_fund = market_state.get_insurance_fund(market_vault_balance);

        let market_data = MarketDataPoint {
            total_collateral: market_state.total_collateral,
            total_user_balances: market_state.total_user_balances,
            total_fee_balance: market_state.total_fee_balance,
            rebalancing_funds: market_state.rebalancing_funds,
            rebalanced_v_coin: market_state.rebalanced_v_coin,
            v_coin_amount: market_state.v_coin_amount,
            v_pc_amount: market_state.v_pc_amount,
            open_shorts_v_coin: market_state.open_shorts_v_coin,
            open_longs_v_coin: market_state.open_longs_v_coin,
            last_funding_timestamp: market_state.last_funding_timestamp,
            last_recording_timestamp: market_state.last_recording_timestamp,
            funding_samples_count: market_state.funding_samples_count,
            funding_samples_sum: market_state.funding_samples_sum,
            funding_history_offset: market_state.funding_history_offset,
            funding_history: market_state.funding_history,
            funding_balancing_factors: market_state.funding_balancing_factors, // FP 32 measure of payment capping to ensure that the insurance fund does not pay funding.
            number_of_instances: market_state.number_of_instances,
            insurance_fund,
            market_price: (market_state.v_pc_amount as f64) / (market_state.v_coin_amount as f64),
            oracle_price: (self.get_oracle_price().await.unwrap() as f64) / (2u64.pow(32) as f64),
            equilibrium_price: ((market_state.v_pc_amount as f64)
                * (market_state.v_coin_amount as f64))
                / (((market_state.v_coin_amount + market_state.open_longs_v_coin
                    - market_state.open_shorts_v_coin) as u128)
                    .pow(2) as f64),
            gc_list_lengths,
            page_full_ratios,
            longs_depths,
            shorts_depths,
        };
        Ok(market_data)
    }

    pub async fn get_market_vault_balance(&mut self) -> Result<u64, ProgramError> {
        let market_vault = self
            .prg_test_ctx
            .banks_client
            .get_account(self.market_ctx.market_vault)
            .await
            .unwrap()
            .unwrap();
        Ok(Account::unpack_from_slice(&market_vault.data)
            .unwrap()
            .amount)
    }

    pub async fn get_instance_address(
        &mut self,
        instance_index: u32,
    ) -> Result<Pubkey, ProgramError> {
        let market_account = self
            .prg_test_ctx
            .banks_client
            .get_account(self.market_ctx.market_account)
            .await
            .unwrap()
            .unwrap();
        let offset = (instance_index as usize)
            .checked_mul(32)
            .and_then(|s| s.checked_add(MarketState::LEN))
            .unwrap();
        let data = market_account.data;
        let slice = data
            .get(offset..offset + 32)
            .ok_or(ProgramError::InvalidArgument)?;
        Ok(Pubkey::new(slice))
    }

    pub async fn parse_instance(
        &mut self,
        instance_address: Pubkey,
    ) -> Result<(Instance, Vec<PageInfo>), ProgramError> {
        let instance_account = self
            .prg_test_ctx
            .banks_client
            .get_account(instance_address)
            .await
            .unwrap()
            .unwrap();
        let header_slice = instance_account
            .data
            .get(0..Instance::LEN)
            .ok_or(ProgramError::InvalidAccountData)?;
        let instance = Instance::unpack_from_slice(header_slice)?;
        let mut offset = Instance::LEN;
        let mut pages = Vec::with_capacity(instance.number_of_pages as usize);
        for _ in 0..instance.number_of_pages {
            let next_offset = offset.checked_add(PageInfo::LEN).unwrap();
            let slice = instance_account
                .data
                .get(offset..next_offset)
                .ok_or(ProgramError::InvalidAccountData)?;
            let page = PageInfo::unpack_from_slice(slice)?;
            pages.push(page);
            offset = next_offset;
        }
        Ok((instance, pages))
    }

    pub async fn update_blockhash(&mut self) -> ProgramResult {
        self.prg_test_ctx.last_blockhash = self
            .prg_test_ctx
            .banks_client
            .get_recent_blockhash()
            .await?;
        Ok(())
    }

    pub async fn get_oracle_price(&mut self) -> Result<u64, ProgramError> {
        let oracle_account = self
            .prg_test_ctx
            .banks_client
            .get_account(self.market_ctx.oracle_account)
            .await
            .unwrap()
            .unwrap();
        Ok(get_oracle_price(
            &oracle_account.data,
            self.test_ctx.coin_decimals,
            self.test_ctx.quote_decimals,
        )?)
    }

    pub async fn get_tree_depth(
        &mut self,
        instance: &Instance,
        mem: &Memory<'_>,
    ) -> (usize, usize) {
        (
            get_tree_depth(instance.longs_pointer, &mem),
            get_tree_depth(instance.shorts_pointer, &mem),
        )
    }

    pub async fn print_tree(&mut self) {
        // Print the tree
        let market_account_data = self
            .prg_test_ctx
            .banks_client
            .get_account(self.market_ctx.market_account)
            .await
            .unwrap()
            .unwrap()
            .data;
        let mut page_data = self
            .prg_test_ctx
            .banks_client
            .get_account(self.market_ctx.instances[0].memory_pages[0])
            .await
            .unwrap()
            .unwrap()
            .data;
        let instance_address = get_instance_address(&market_account_data, 0).unwrap();
        let instance_data = self
            .prg_test_ctx
            .banks_client
            .get_account(instance_address)
            .await
            .unwrap()
            .unwrap()
            .data;
        let (instance, page_infos) = parse_instance(&instance_data).unwrap();
        let pages = vec![Page {
            page_size: (page_data.len() / SLOT_SIZE) as u32,
            data: Rc::new(RefCell::new(&mut page_data)),
            uninitialized_memory: page_infos[0].unitialized_memory_index,
            free_slot_list_hd: page_infos[0].free_slot_list_hd,
        }];
        let mem = Memory::new(pages, instance.garbage_pointer);

        println!("Tree: LONGS TREE");
        log::info!("Tree: LONGS TREE");
        if let Some(longs_pt) = instance.longs_pointer {
            print_tree(longs_pt, &mem, 0);
        }

        println!("Tree: SHORTS TREE");
        log::info!("Tree: SHORTS TREE");
        if let Some(shorts_pt) = instance.shorts_pointer {
            print_tree(shorts_pt, &mem, 0);
        }
    }
}
