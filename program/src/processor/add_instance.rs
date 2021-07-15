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
        instance::{write_instance, write_page_info, Instance, PageInfo},
        is_initialized,
        market::{write_instance_address, MarketState},
    },
    utils::{check_account_owner, check_signer},
};

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    admin: &'a AccountInfo<'b>,
    instance: &'a AccountInfo<'b>,
    memory_pages: &'a [AccountInfo<'b>],
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

        let memory_pages = &accounts
            .get(3..)
            .ok_or(ProgramError::NotEnoughAccountKeys)
            .unwrap();

        check_signer(admin).unwrap();
        check_account_owner(instance, program_id).unwrap();
        check_account_owner(market, program_id).unwrap();

        if is_initialized(instance) {
            msg!("Instance account is already initialized!");
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(Self {
            market,
            admin,
            instance,
            memory_pages,
        })
    }
}

pub fn process_add_instance(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    for page_idx in 0..accounts.memory_pages.len() {
        let page_info = PageInfo::new(accounts.memory_pages[page_idx].key);
        write_page_info(
            &mut accounts.instance.data.borrow_mut(),
            page_idx,
            &page_info,
        )?;
    }

    let instance = Instance {
        version: 0,
        shorts_pointer: None,
        longs_pointer: None,
        garbage_pointer: None,
        number_of_pages: accounts.memory_pages.len() as u32,
    };

    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    if &Pubkey::new(&market_state.admin_address) != accounts.admin.key {
        msg!("Invalid admin account for the current market");
        return Err(ProgramError::InvalidArgument);
    }

    write_instance_address(
        &mut accounts.market.data.borrow_mut(),
        market_state.number_of_instances,
        &accounts.instance.key,
    )?;
    write_instance(&mut accounts.instance.data.borrow_mut(), &instance)?;
    market_state.number_of_instances += 1;

    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());

    Ok(())
}
