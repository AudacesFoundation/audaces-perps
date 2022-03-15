use solana_program::{
    instruction::{Instruction, InstructionError},
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
};
use solana_program_test::{BanksClientError, ProgramTestContext};
use solana_sdk::{signature::Keypair, transaction::Transaction, transport::TransportError};
use solana_sdk::{signature::Signer, transaction::TransactionError};
use spl_associated_token_account::{create_associated_token_account, get_associated_token_address};
use spl_token::instruction::initialize_mint;

// Utils
pub async fn sign_send_instructions(
    ctx: &mut ProgramTestContext,
    instructions: Vec<Instruction>,
    signers: Vec<&Keypair>,
) -> Result<(), BanksClientError> {
    let mut transaction = Transaction::new_with_payer(&instructions, Some(&ctx.payer.pubkey()));
    let mut payer_signers = vec![&ctx.payer];
    for s in signers {
        payer_signers.push(s);
    }
    transaction.partial_sign(&payer_signers, ctx.last_blockhash);
    ctx.banks_client.process_transaction(transaction).await
}

pub fn create_and_get_associated_token_address(
    ctx: &ProgramTestContext,
    parent_key: &Pubkey,
    mint_key: &Pubkey,
) -> (Transaction, Pubkey) {
    let instruction = create_associated_token_account(&ctx.payer.pubkey(), parent_key, mint_key);
    let asset_key = get_associated_token_address(parent_key, mint_key);
    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&ctx.payer.pubkey()));
    transaction.partial_sign(&[&ctx.payer], ctx.last_blockhash);
    (transaction, asset_key)
}

pub fn mint_init_transaction(
    ctx: &ProgramTestContext,
    mint: &Keypair,
    mint_authority: &Keypair,
) -> Transaction {
    let instructions = [
        system_instruction::create_account(
            &ctx.payer.pubkey(),
            &mint.pubkey(),
            Rent::default().minimum_balance(82),
            82,
            &spl_token::id(),
        ),
        initialize_mint(
            &spl_token::id(),
            &mint.pubkey(),
            &mint_authority.pubkey(),
            None,
            6,
        )
        .unwrap(),
    ];
    let mut transaction = Transaction::new_with_payer(&instructions, Some(&ctx.payer.pubkey()));
    transaction.partial_sign(&[&ctx.payer, mint], ctx.last_blockhash);
    transaction
}

#[allow(clippy::collapsible_match)]
pub fn catch_noop(err: BanksClientError) -> Result<(), InstructionError> {
    match err {
        BanksClientError::TransactionError(te) => match te {
            TransactionError::InstructionError(_, ie) => match ie {
                InstructionError::Custom(7) => Ok(()),
                _ => Err(ie),
            },
            _ => {
                panic!()
            }
        },
        _ => {
            panic!()
        }
    }
}
