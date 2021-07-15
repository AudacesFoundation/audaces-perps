use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
};

use crate::{
    state::{
        instance::{parse_instance, write_instance, write_page_info, PageInfo},
        is_initialized,
        market::{get_instance_address, MarketState},
    },
    utils::{check_account_owner, check_signer},
};

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    admin: &'a AccountInfo<'b>,
    instance: &'a AccountInfo<'b>,
    new_memory_page: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let market = next_account_info(accounts_iter)?;
        let admin = next_account_info(accounts_iter)?;
        let instance = next_account_info(accounts_iter)?;
        let new_memory_page = next_account_info(accounts_iter)?;

        check_signer(admin).unwrap();
        check_account_owner(new_memory_page, program_id).unwrap();
        check_account_owner(market, program_id).unwrap();
        check_account_owner(instance, program_id).unwrap();

        if is_initialized(new_memory_page) {
            msg!("Memory page account is already initialized!");
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(Self {
            market,
            admin,
            instance,
            new_memory_page,
        })
    }
}

pub fn process_add_page(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instance_index: u8,
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let instance_address =
        get_instance_address(&accounts.market.data.borrow(), instance_index as u32)?;
    let market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;
    // Verifications
    if &instance_address != accounts.instance.key {
        msg!("Invalid instance account or instance index provided");
        return Err(ProgramError::InvalidArgument);
    }
    if &Pubkey::new(&market_state.admin_address) != accounts.admin.key {
        msg!("Invalid admin account for the current market");
        return Err(ProgramError::InvalidArgument);
    }

    let (mut instance, _) = parse_instance(&accounts.instance.data.borrow())?;

    let page_info = PageInfo::new(accounts.new_memory_page.key);
    write_page_info(
        &mut accounts.instance.data.borrow_mut(),
        instance.number_of_pages as usize,
        &page_info,
    )?;

    instance.number_of_pages += 1;

    write_instance(&mut accounts.instance.data.borrow_mut(), &instance)?;

    Ok(())
}
