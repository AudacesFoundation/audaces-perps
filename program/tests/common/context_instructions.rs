use super::utils::sign_send_instructions;
use crate::common::context::Context;
use audaces_protocol::{
    instruction::{
        add_budget, add_instance, add_page, close_account, close_position, collect_garbage,
        crank_funding, crank_liquidation, create_market, extract_funding, increase_position,
        open_position, rebalance, transfer_position, transfer_user_account, withdraw_budget,
    },
    instruction::{InstanceContext, PositionInfo},
    state::PositionType,
};
use solana_program::{pubkey::Pubkey, system_instruction::create_account};
use solana_sdk::{signature::Keypair, signer::Signer, transport::TransportError};

impl Context {
    pub async fn create_market(
        &mut self,
        market_symbol: String,
        initial_v_pc_amount: u64,
        coin_decimals: u8,
        quote_decimals: u8,
    ) -> Result<(), TransportError> {
        let create_market_instruction = create_market(
            &self.market_ctx,
            market_symbol,
            initial_v_pc_amount,
            coin_decimals,
            quote_decimals,
        );
        sign_send_instructions(
            &mut self.prg_test_ctx,
            vec![create_market_instruction],
            vec![],
        )
        .await
    }

    pub async fn add_instance(
        &mut self,
        nb_pages_per_instance: u8,
        space_per_page: u64,
    ) -> Result<(), TransportError> {
        let instance_keypair = Keypair::new();
        let mut instructions = vec![create_account(
            &self.prg_test_ctx.payer.pubkey(),
            &instance_keypair.pubkey(),
            1_000_000,
            1_000_000,
            &self.market_ctx.audaces_protocol_program_id,
        )];
        let mut signers = vec![];
        let mut signers_ref = vec![&instance_keypair];
        let mut pages_pubkeys = vec![];
        for _ in 0..nb_pages_per_instance {
            let page_keypair = Keypair::new();
            pages_pubkeys.push(page_keypair.pubkey());
            instructions.push(create_account(
                &self.prg_test_ctx.payer.pubkey(),
                &page_keypair.pubkey(),
                1_000_000,
                space_per_page,
                &self.market_ctx.audaces_protocol_program_id,
            ));
            signers.push(page_keypair);
        }
        instructions.push(add_instance(
            &self.market_ctx,
            instance_keypair.pubkey(),
            &pages_pubkeys,
        ));
        signers_ref.push(&self.test_ctx.market_admin_keypair);

        self.market_ctx.instances.push(InstanceContext {
            instance_account: instance_keypair.pubkey(),
            memory_pages: pages_pubkeys,
        });

        sign_send_instructions(
            &mut self.prg_test_ctx,
            instructions,
            signers.iter().chain(signers_ref).collect::<Vec<&Keypair>>(),
        )
        .await
    }

    pub async fn add_budget(
        &mut self,
        amount: u64,
        user_account_index: usize,
    ) -> Result<(), TransportError> {
        let add_budget_instruction = add_budget(
            &self.market_ctx,
            amount,
            self.user_ctx.owner_account.pubkey(),
            self.user_ctx.usdc_account,
            self.user_ctx.user_accounts[user_account_index],
        );
        sign_send_instructions(
            &mut self.prg_test_ctx,
            vec![add_budget_instruction],
            vec![&self.user_ctx.owner_account],
        )
        .await
    }

    pub async fn withdraw_budget(
        &mut self,
        amount: u64,
        user_account_index: usize,
    ) -> Result<(), TransportError> {
        let withdraw_budget_instruction = withdraw_budget(
            &self.market_ctx,
            amount,
            self.user_ctx.usdc_account,
            self.user_ctx.owner_account.pubkey(),
            self.user_ctx.user_accounts[user_account_index],
        );
        sign_send_instructions(
            &mut self.prg_test_ctx,
            vec![withdraw_budget_instruction],
            vec![&self.user_ctx.owner_account],
        )
        .await
    }

    pub async fn open_position(
        &mut self,
        side: PositionType,
        collateral: u64,
        leverage: u64,
        instance_index: u8,
        user_account_index: usize,
    ) -> Result<(), TransportError> {
        let open_position_instruction = open_position(
            &self.market_ctx,
            &PositionInfo {
                user_account: self.user_ctx.user_accounts[user_account_index],
                user_account_owner: self.user_ctx.owner_account.pubkey(),
                instance_index,
                side,
            },
            collateral,
            leverage,
            0,
            u64::MAX,
            None,
            None,
        );
        sign_send_instructions(
            &mut self.prg_test_ctx,
            vec![open_position_instruction],
            vec![&self.user_ctx.owner_account],
        )
        .await
    }

    pub async fn increase_position(
        &mut self,
        collateral: u64,
        leverage: u64,
        position_index: u16,
        instance_index: u8,
        user_account_index: usize,
    ) -> Result<(), TransportError> {
        let increase_position_instruction = increase_position(
            &self.market_ctx,
            collateral,
            leverage,
            instance_index,
            position_index,
            self.user_ctx.owner_account.pubkey(),
            self.user_ctx.user_accounts[user_account_index],
            0,
            u64::MAX,
            None,
            None,
        );
        sign_send_instructions(
            &mut self.prg_test_ctx,
            vec![increase_position_instruction],
            vec![&self.user_ctx.owner_account],
        )
        .await
    }

    pub async fn close_position(
        &mut self,
        closing_collateral: u64,
        closing_v_coin: u64,
        position_index: u16,
        user_account_index: usize,
    ) -> Result<(), TransportError> {
        let position = self
            .get_position(position_index, user_account_index)
            .await
            .unwrap();
        let close_position_instruction = close_position(
            &self.market_ctx,
            &PositionInfo {
                user_account: self.user_ctx.user_accounts[user_account_index],
                user_account_owner: self.user_ctx.owner_account.pubkey(),
                instance_index: position.instance_index,
                side: position.side,
            },
            closing_collateral,
            closing_v_coin,
            position_index,
            0,
            u64::MAX,
            None,
            None,
        );
        sign_send_instructions(
            &mut self.prg_test_ctx,
            vec![close_position_instruction],
            vec![&self.user_ctx.owner_account],
        )
        .await
    }

    pub async fn liquidate(&mut self, instance_index: u8) -> Result<(), TransportError> {
        let liquidate_instruction =
            crank_liquidation(&self.market_ctx, instance_index, self.user_ctx.usdc_account);
        sign_send_instructions(&mut self.prg_test_ctx, vec![liquidate_instruction], vec![]).await
    }

    pub async fn collect_garbage(
        &mut self,
        instance_index: u8,
        max_iterations: u64,
    ) -> Result<(), TransportError> {
        let collect_garbage_instruction = collect_garbage(
            &self.market_ctx,
            instance_index,
            max_iterations,
            self.user_ctx.usdc_account,
        );
        sign_send_instructions(
            &mut self.prg_test_ctx,
            vec![collect_garbage_instruction],
            vec![],
        )
        .await
    }

    pub async fn crank_funding(&mut self) -> Result<(), TransportError> {
        let crank_funding_instruction = crank_funding(&self.market_ctx);
        sign_send_instructions(
            &mut self.prg_test_ctx,
            vec![crank_funding_instruction],
            vec![],
        )
        .await
    }

    pub async fn extract_funding(
        &mut self,
        instance_index: u8,
        user_account_index: usize,
    ) -> Result<(), TransportError> {
        let crank_funding_instruction = extract_funding(
            &self.market_ctx,
            instance_index,
            self.user_ctx.user_accounts[user_account_index],
        );
        sign_send_instructions(
            &mut self.prg_test_ctx,
            vec![crank_funding_instruction],
            vec![],
        )
        .await
    }

    pub async fn close_account(
        &mut self,
        lamports_target: Pubkey,
        user_account_index: usize,
    ) -> Result<(), TransportError> {
        let close_account_instruction = close_account(
            &self.market_ctx,
            self.user_ctx.user_accounts[user_account_index],
            self.user_ctx.owner_account.pubkey(),
            lamports_target,
        );

        sign_send_instructions(
            &mut self.prg_test_ctx,
            vec![close_account_instruction],
            vec![&self.user_ctx.owner_account],
        )
        .await
    }

    pub async fn add_page(&mut self, instance_index: u8, space: u64) -> Result<(), TransportError> {
        let page_keypair = Keypair::new();

        let instructions = vec![
            create_account(
                &self.prg_test_ctx.payer.pubkey(),
                &page_keypair.pubkey(),
                1_000_000,
                space,
                &self.market_ctx.audaces_protocol_program_id,
            ),
            add_page(&self.market_ctx, instance_index, page_keypair.pubkey()),
        ];
        let signers = vec![&page_keypair, &self.test_ctx.market_admin_keypair];

        self.market_ctx.instances[instance_index as usize]
            .memory_pages
            .push(page_keypair.pubkey());

        sign_send_instructions(&mut self.prg_test_ctx, instructions, signers).await
    }

    pub async fn rebalance(
        &mut self,
        instance_index: u8,
        collateral: u64,
        user_account_index: usize,
    ) -> Result<(), TransportError> {
        let instructions = vec![rebalance(
            &self.market_ctx,
            self.user_ctx.user_accounts[user_account_index],
            self.user_ctx.owner_account.pubkey(),
            instance_index,
            collateral,
        )];
        let signers = vec![
            &self.user_ctx.owner_account,
            &self.test_ctx.market_admin_keypair,
        ];
        sign_send_instructions(&mut self.prg_test_ctx, instructions, signers).await
    }

    pub async fn transfer_user_account(
        &mut self,
        new_user_account_owner: Keypair,
        user_account_index: usize,
    ) -> Result<(), TransportError> {
        let instructions = vec![transfer_user_account(
            &self.market_ctx,
            self.user_ctx.user_accounts[user_account_index],
            self.user_ctx.owner_account.pubkey(),
            new_user_account_owner.pubkey(),
        )];
        let signers = vec![&self.user_ctx.owner_account];
        let r = sign_send_instructions(&mut self.prg_test_ctx, instructions, signers).await;
        self.user_ctx.owner_account = new_user_account_owner;
        return r;
    }

    pub async fn transfer_position_to_new_user(
        &mut self,
        position_index: u16,
        user_account_index: usize,
    ) -> Result<(), TransportError> {
        let new_user_account = Keypair::new();

        let create_instruction = vec![create_account(
            &self.prg_test_ctx.payer.pubkey(),
            &new_user_account.pubkey(),
            1_000_000,
            1_000_000,
            &self.market_ctx.audaces_protocol_program_id,
        )];
        let signers = vec![&new_user_account];
        sign_send_instructions(&mut self.prg_test_ctx, create_instruction, signers)
            .await
            .unwrap();

        let transfer_instruction = vec![transfer_position(
            &self.market_ctx,
            position_index,
            self.user_ctx.user_accounts[user_account_index],
            self.user_ctx.owner_account.pubkey(),
            new_user_account.pubkey(),
            self.user_ctx.owner_account.pubkey(),
        )];

        self.user_ctx.user_accounts.push(new_user_account.pubkey());

        self.add_budget(10_000_000, self.user_ctx.user_accounts.len() - 1)
            .await
            .unwrap();

        let signers = vec![&self.user_ctx.owner_account];
        let r = sign_send_instructions(&mut self.prg_test_ctx, transfer_instruction, signers).await;
        return r;
    }

    pub async fn create_user_accounts(
        &mut self,
        nb_new_accounts: usize,
    ) -> Result<(), TransportError> {
        let mut instructions = vec![];
        let mut signers: Vec<Keypair> = vec![];
        for _ in 0..nb_new_accounts {
            let new_user_account = Keypair::new();

            instructions.push(create_account(
                &self.prg_test_ctx.payer.pubkey(),
                &new_user_account.pubkey(),
                1_000_000,
                1_000_000,
                &self.market_ctx.audaces_protocol_program_id,
            ));
            signers.push(new_user_account);
        }
        let signers_ref: Vec<&Keypair> = signers.iter().collect();
        self.user_ctx
            .user_accounts
            .append(&mut signers.iter().map(|k| k.pubkey()).collect());
        sign_send_instructions(&mut self.prg_test_ctx, instructions, signers_ref).await
    }
}
