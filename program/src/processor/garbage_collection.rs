use std::slice::Iter;

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
};
use spl_token::instruction::transfer;

use crate::{
    error::PerpError,
    positions_book::{memory::parse_memory, positions_book_tree::PositionsBook},
    state::{
        instance::{parse_instance, write_instance_and_memory},
        market::{get_instance_address, MarketState},
    },
    utils::{check_account_key, check_account_owner},
};

use super::ALLOCATION_FEE;

pub struct Accounts<'a, 'b: 'a> {
    spl_token_program: &'a AccountInfo<'b>,
    market: &'a AccountInfo<'b>,
    instance: &'a AccountInfo<'b>,
    market_vault: &'a AccountInfo<'b>,
    market_signer: &'a AccountInfo<'b>,
    target_fee: &'a AccountInfo<'b>,
    remaining: Iter<'a, AccountInfo<'b>>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let mut accounts_iter = accounts.iter();

        let spl_token_program = next_account_info(&mut accounts_iter)?;
        let market = next_account_info(&mut accounts_iter)?;
        let instance = next_account_info(&mut accounts_iter)?;
        let market_vault = next_account_info(&mut accounts_iter)?;
        let market_signer = next_account_info(&mut accounts_iter)?;
        let target_fee = next_account_info(&mut accounts_iter)?;

        check_account_key(spl_token_program, &spl_token::id()).unwrap();
        check_account_owner(market, program_id).unwrap();

        Ok(Self {
            spl_token_program,
            market,
            instance,
            market_vault,
            market_signer,
            target_fee,
            remaining: accounts_iter,
        })
    }
}

pub fn process_garbage_collection(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instance_index: u8,
    max_iterations: u64,
) -> ProgramResult {
    let mut accounts = Accounts::parse(program_id, accounts)?;

    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    let instance_address =
        get_instance_address(&accounts.market.data.borrow(), instance_index as u32)?;
    if &instance_address != accounts.instance.key {
        msg!("Invalid instance account or instance index provided");
        return Err(ProgramError::InvalidArgument);
    }

    let (mut instance, mut page_infos) = parse_instance(&accounts.instance.data.borrow())?;
    let memory = parse_memory(&instance, &page_infos, &mut accounts.remaining)?;
    let mut book = PositionsBook::new(instance.shorts_pointer, instance.longs_pointer, memory);

    let freed_slots = book.memory.crank_garbage_collector(max_iterations)?;

    if freed_slots == 0 {
        msg!("No slots to collect.");
        return Err(PerpError::Nop.into());
    }

    instance.garbage_pointer = book.memory.gc_list_hd;

    let reward = freed_slots * ALLOCATION_FEE;

    let instruction = transfer(
        &spl_token::id(),
        accounts.market_vault.key,
        accounts.target_fee.key,
        accounts.market_signer.key,
        &[],
        reward,
    )?;

    invoke_signed(
        &instruction,
        &[
            accounts.spl_token_program.clone(),
            accounts.market_vault.clone(),
            accounts.target_fee.clone(),
            accounts.market_signer.clone(),
        ],
        &[&[
            &accounts.market.key.to_bytes(),
            &[market_state.signer_nonce],
        ]],
    )?;

    market_state.total_fee_balance = market_state.total_fee_balance.checked_sub(reward).unwrap();

    instance.update(&book, &mut page_infos);
    write_instance_and_memory(
        &mut accounts.instance.data.borrow_mut(),
        &page_infos,
        &instance,
    )?;
    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());

    Ok(())
}
