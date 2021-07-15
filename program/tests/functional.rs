use audaces_protocol::state::PositionType;
use solana_program::pubkey::Pubkey;
use solana_sdk::signer::keypair::Keypair;
pub mod common;
use crate::common::{context::Context, utils::catch_noop};

#[tokio::test]
async fn test_audaces_protocol() {
    // Set up testing and market context
    let mut context = Context::init(0, 6, 6).await;

    // Set up the oracle price
    context.change_oracle_price(10_000 << 32u64).await.unwrap();

    // Begin program interaction
    context
        .create_market("BTC/USD".to_string(), 1e10f64 as u64, 6, 6)
        .await
        .unwrap();

    context.add_instance(1, 1_000_000).await.unwrap();

    context.add_page(0, 1_000_000).await.unwrap();

    context.add_budget(5_000_000, 0).await.unwrap();

    context.withdraw_budget(1_000_000, 0).await.unwrap();

    println!("{:?}", context.get_market_state().await.unwrap());

    context
        .open_position(PositionType::Long, 1_000_000, 10 << 32u64, 0, 0)
        .await
        .unwrap();

    context.print_tree().await;

    context
        .increase_position(1_000_000, 10 << 32u64, 0, 0, 0)
        .await
        .unwrap();

    context.print_tree().await;

    let open_position = context.get_position(0, 0).await.unwrap();
    println!("open_position: {:x?}", open_position);

    context
        .close_position(1_000_000, open_position.v_coin_amount / 2, 0, 0)
        .await
        .unwrap();

    println!("Before liquidation");
    context.print_tree().await;

    // Change the oracle price to provoke liquidation
    context.change_oracle_price(1 << 32u64).await.unwrap();

    if let Err(err) = context.liquidate(0).await {
        catch_noop(err).unwrap();
    }

    if let Err(err) = context.collect_garbage(0, 100).await {
        catch_noop(err).unwrap();
    }

    if let Err(err) = context.crank_funding().await {
        catch_noop(err).unwrap();
    }

    if let Err(err) = context.extract_funding(0, 0).await {
        catch_noop(err).unwrap();
    }

    println!("After liquidation");
    context.print_tree().await;

    let user_account = context.get_user_account(0).await.unwrap();
    let market_state = context.get_market_state().await.unwrap();

    context
        .close_position(u64::MAX, u64::MAX, 0, 0)
        .await
        .unwrap();

    context.add_budget(10_000_000, 0).await.unwrap();

    context
        .open_position(PositionType::Long, 2_000_000, 10 << 32u64, 0, 0)
        .await
        .unwrap();
    println!(
        "Imbalance : {}",
        (market_state.open_longs_v_coin as i64) - (market_state.open_shorts_v_coin as i64)
    );

    println!("Before rebalance:");
    context.print_tree().await;

    context.rebalance(0, user_account.balance, 0).await.unwrap();
    println!("After rebalance:");
    context.print_tree().await;
    context.prg_test_ctx.warp_to_slot(3).unwrap();

    context
        .close_position(u64::MAX, u64::MAX, 0, 0)
        .await
        .unwrap();
    context.prg_test_ctx.warp_to_slot(5).unwrap();
    context
        .close_position(u64::MAX, u64::MAX, 0, 0)
        .await
        .unwrap();

    context
        .open_position(PositionType::Long, 1_000_000, 10 << 32u64, 0, 0)
        .await
        .unwrap();

    println!("{:?}", context.get_user_account(0).await.unwrap());
    context.transfer_position_to_new_user(0, 0).await.unwrap();
    println!("{:?}", context.get_user_account(0).await.unwrap());

    context
        .close_position(u64::MAX, u64::MAX, 0, 0)
        .await
        .unwrap();

    let user_account = context.get_user_account(0).await.unwrap();
    context
        .withdraw_budget(user_account.balance, 0)
        .await
        .unwrap();

    context
        .transfer_user_account(Keypair::new(), 0)
        .await
        .unwrap();

    context
        .close_account(Pubkey::new_unique(), 0)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_overflow_0() {
    // Set up testing and market context
    let mut context = Context::init(0, 6, 6).await;

    // Set up the oracle price
    context.change_oracle_price(8500 << 32u64).await.unwrap();

    // Begin program interaction
    context
        .create_market("BTC/USD".to_string(), 1e9f64 as u64, 6, 6)
        .await
        .unwrap();

    context.add_instance(1, 1_000_000).await.unwrap();

    context.add_budget(10_000_000, 0).await.unwrap();

    context
        .open_position(PositionType::Long, 1_000_000, 10 << 32u64, 0, 0)
        .await
        .unwrap();

    context.print_tree().await;

    context
        .increase_position(1_000_000, 10 << 32u64, 0, 0, 0)
        .await
        .unwrap();

    context.print_tree().await;

    let open_position = context.get_position(0, 0).await.unwrap();

    context
        .close_position(2_000_000, open_position.v_coin_amount, 0, 0)
        .await
        .unwrap();

    // Change the oracle price to provoke liquidation
    context.change_oracle_price(1 << 32u64).await.unwrap();

    if let Err(err) = context.liquidate(0).await {
        catch_noop(err).unwrap();
    }

    if let Err(err) = context.collect_garbage(0, 100).await {
        catch_noop(err).unwrap();
    }

    if let Err(err) = context.crank_funding().await {
        catch_noop(err).unwrap();
    }

    if let Err(err) = context.extract_funding(0, 0).await {
        catch_noop(err).unwrap();
    }

    let state = context.get_market_state().await.unwrap();
    println!("market_state : {:#?}", state);
}
