#![cfg(not(feature = "test-bpf"))]

pub mod common;

use crate::common::{context::Context, utils::catch_noop};

#[derive(Debug)]
pub struct GlobalVars {
    prob_open_threshold: f64,
    liquidation_prob: f64,
    oracle_price_var_mean: f64,
    oracle_price_var_stdv: f64,
    collateral_mean: f64,
    collateral_stdv: f64,
    leverage_mean: f64,
    leverage_stdv: f64,
    nb_instructions: usize,
    initial_oracle_price: u64, // FP32
    initial_vpc_amount: u64,
    max_price_diff: f64,
    scenario: ScenarioType,
}

static GLOBAL_VARS: GlobalVars = GlobalVars {
    prob_open_threshold: 0.8, // Probability
    liquidation_prob: 0.8,    // Probability
    oracle_price_var_mean: 3.0,
    oracle_price_var_stdv: 15.0,
    collateral_mean: 1_000_000_000.0,
    collateral_stdv: 500_000.0,
    leverage_mean: 5.0,
    leverage_stdv: 3.0,
    nb_instructions: 1_000,
    initial_oracle_price: 1000 << 32, // FP32
    initial_vpc_amount: 1e12 as u64,
    max_price_diff: 10.,
    scenario: ScenarioType::Crash,
};

#[derive(Debug, PartialEq)]
pub enum ScenarioType {
    // Random,
    Monotone,
    Turn,
    Crash,
    MultUserAccounts,
}

#[test]
fn simulation() {
    use audaces_protocol::state::PositionType;
    use log::LevelFilter;
    use log4rs::append::file::FileAppender;
    use log4rs::config::{Appender, Config, Root};
    use log4rs::encode::pattern::PatternEncoder;
    use rand::{prelude::StdRng, Rng, SeedableRng};
    use rand_distr::{Distribution, Normal, Uniform};
    use solana_program::instruction::InstructionError;
    use solana_sdk::{transaction::TransactionError, transport::TransportError};

    // Logging
    let logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{l} - {m}\n")))
        .build("tests/integration_tests_output/log/output.log")
        .unwrap();
    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .build(
            Root::builder()
                .appender("logfile")
                .build(LevelFilter::Debug),
        )
        .unwrap();
    log4rs::init_config(config).unwrap();
    log::info!("Initial Conditions: {:?}", GLOBAL_VARS);

    // RNG
    let mut seed = 0;
    if seed == 0 {
        let seed_rng = &mut rand::thread_rng();
        seed = seed_rng.sample(Uniform::new(0, u64::MAX));
    }
    let rng = &mut StdRng::seed_from_u64(seed);
    log::info!("Seed for this run: {:?}", seed);
    let mut slot = 1;

    // Set up test env
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut context = rt.block_on(Context::init(0, 6, 6));
    rt.block_on(context.change_oracle_price(GLOBAL_VARS.initial_oracle_price))
        .unwrap();
    let mut previous_oracle_price = GLOBAL_VARS.initial_oracle_price;
    rt.block_on(context.create_market("".to_string(), GLOBAL_VARS.initial_vpc_amount, 6, 6))
        .unwrap();
    rt.block_on(context.add_instance(5, 10_000)).unwrap();

    let oracle_price_var_distr = Normal::new(
        GLOBAL_VARS.oracle_price_var_mean,
        GLOBAL_VARS.oracle_price_var_stdv,
    )
    .unwrap();
    let collateral_distr =
        Normal::new(GLOBAL_VARS.collateral_mean, GLOBAL_VARS.collateral_stdv).unwrap();
    let leverage_distr = Normal::new(GLOBAL_VARS.leverage_mean, GLOBAL_VARS.leverage_stdv).unwrap();

    for i in 0..GLOBAL_VARS.nb_instructions as usize {
        // Get and log Market data
        let market_data = rt.block_on(context.get_market_data()).unwrap();
        log::info!("{:?}", market_data);

        // Construct the main instruction
        let uniform = Uniform::new(0., 1.);
        let result = match GLOBAL_VARS.scenario {
            ScenarioType::MultUserAccounts => {
                if i == 0 {
                    rt.block_on(context.create_user_accounts(200)).unwrap();
                    rt.block_on(context.add_budget(1_000_000_000, 0))
                } else if i < 200 {
                    rt.block_on(context.add_budget(1_000_000_000, i))
                } else {
                    rt.block_on(context.withdraw_budget(1_000_000_000, i - 200))
                }
            }
            _ => {
                if i == 0 {
                    rt.block_on(context.add_budget(1 << 60, 0))
                } else {
                    let market_price = market_data.v_pc_amount / market_data.v_coin_amount;
                    let prob_long_threshold = (((((previous_oracle_price >> 32) as f64)
                        - (market_price as f64))
                        / GLOBAL_VARS.max_price_diff)
                        + 1.)
                        / 2.;

                    let collateral = collateral_distr.sample(rng) as u64;
                    let leverage = (leverage_distr.sample(rng) as u64) << 32;

                    if uniform.sample(rng) < GLOBAL_VARS.prob_open_threshold {
                        rt.block_on(context.open_position(
                            match uniform.sample(rng) < prob_long_threshold {
                                true => PositionType::Long,
                                false => PositionType::Short,
                            },
                            collateral,
                            leverage,
                            0, // TODO
                            0,
                        ))
                    } else {
                        rt.block_on(context.close_position(
                            collateral,
                            (((collateral as u128) * (leverage as u128)) >> 32) as u64,
                            0,
                            0,
                        ))
                    }
                }
            }
        };

        // Execute
        match result {
            Ok(_) => {}
            Err(TransportError::TransactionError(te)) => match te {
                TransactionError::InstructionError(_, ie) => match ie {
                    InstructionError::InvalidArgument
                    | InstructionError::Custom(2)
                    | InstructionError::Custom(4)
                    | InstructionError::Custom(6)
                    | InstructionError::Custom(11) => {
                        log::error!("{:?}", ie)
                    }
                    _ => {
                        log::error!("{:?}", ie);
                        Err(ie).unwrap()
                    }
                },
                _ => {
                    log::error!("{:?}", te);
                    panic!()
                }
            },
            Err(e) => {
                log::error!("{:?}", e);
                panic!()
            }
        }

        // Increment slot
        slot += 2;
        context.prg_test_ctx.warp_to_slot(slot).unwrap();

        // Liquidate if so
        if uniform.sample(rng) < GLOBAL_VARS.liquidation_prob {
            if let Err(err) = rt.block_on(context.liquidate(0)) {
                catch_noop(err).unwrap();
            }
        }
        //Funding
        if let Err(err) = rt.block_on(context.crank_funding()) {
            catch_noop(err).unwrap();
        }
        if let Err(err) = rt.block_on(context.extract_funding(0, 0)) {
            catch_noop(err).unwrap();
        }

        // Garbage collection
        if let Err(err) = rt.block_on(context.collect_garbage(0, 100)) {
            catch_noop(err).unwrap();
        }

        // Update oracle price depending on the scenario
        let new_oracle_price;
        if GLOBAL_VARS.scenario == ScenarioType::Crash
            && i >= GLOBAL_VARS.nb_instructions / 2
            && i < 8 + GLOBAL_VARS.nb_instructions / 2
        {
            new_oracle_price = (((previous_oracle_price >> 32) as f64
                - 10. * oracle_price_var_distr.sample(rng).abs())
                as u64)
                << 32;
        } else {
            new_oracle_price = (((previous_oracle_price >> 32) as f64
                + oracle_price_var_distr.sample(rng)) as u64)
                << 32;
        }
        // if i % 3 == 0 {
        rt.block_on(context.change_oracle_price(new_oracle_price))
            .unwrap();
        previous_oracle_price = new_oracle_price;
        // }

        // High cost
        // rt.block_on(context.print_tree());

        // Increment slot
        slot += 2;
        context.prg_test_ctx.warp_to_slot(slot).unwrap();

        // Print Progress
        if i % (GLOBAL_VARS.nb_instructions / 50) == 0 {
            // Printing to stderr for cleaner redirection in terminal
            eprintln!(
                "Progress: {:?} %",
                100 * (i as u64) / GLOBAL_VARS.nb_instructions as u64
            );
        }

        rt.block_on(context.update_blockhash()).unwrap();
    }
}
