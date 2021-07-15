use clap::{value_t_or_exit, App, Arg, SubCommand};
use perps_crank::Context;
use solana_clap_utils::{
    fee_payer::{fee_payer_arg, FEE_PAYER_ARG},
    input_parsers::{keypair_of, pubkey_of},
    input_validators::is_pubkey,
};

fn main() {
    let default_threads = num_cpus::get().to_string();
    let matches = App::new("perps-crank")
        .version("0.1")
        .author("Audaces Protocol")
        .about("Distributed Audaces Protocol cranking runtime")
        .subcommand(SubCommand::with_name("liquidate").about("Crank liquidation operations"))
        .subcommand(SubCommand::with_name("funding").about("Crank liquidation operations"))
        .subcommand(
            SubCommand::with_name("garbage-collect").about("Crank garbage collection operations"),
        )
        .subcommand(
            SubCommand::with_name("funding-extraction")
                .about("Crank funding extraction operations")
                .arg(
                    Arg::with_name("swarm_size")
                        .long("swarm-size")
                        .help("The number of nodes in the current cranking swarm")
                        .takes_value(true)
                        .default_value("1")
                        .validator(|s| {
                            s.parse::<u32>()
                                .map(|_| ())
                                .map_err(|_| String::from("The swarm size must be an integer"))
                        }),
                )
                .arg(
                    Arg::with_name("node_id")
                        .long("node-id")
                        .help("The integer node identifer within the swarm")
                        .takes_value(true)
                        .default_value("0")
                        .validator(|s| {
                            s.parse::<u32>().map(|_| ()).map_err(|_| {
                                String::from("The integer node identifer  must be an integer")
                            })
                        }),
                ),
        )
        .arg(
            Arg::with_name("url")
                .short("u")
                .long("url")
                .help("A Solana RPC endpoint url")
                .takes_value(true),
        )
        .arg(fee_payer_arg())
        .arg(
            Arg::with_name("program_id")
                .short("p")
                .long("program-id")
                .help("The pubkey of the Audaces Protocol program")
                .takes_value(true)
                .validator(is_pubkey)
                .required(true),
        )
        .arg(
            Arg::with_name("market")
                .short("m")
                .long("market")
                .help("The pubkey of the Audaces Protocol market to interact with")
                .takes_value(true)
                .validator(is_pubkey)
                .required(true),
        )
        .arg(
            Arg::with_name("threads")
                .short("n")
                .long("num-threads")
                .help("The number of CPU threads to use for multithreaded tasks")
                .takes_value(true)
                .default_value(&default_threads),
        )
        .get_matches();
    let endpoint = matches
        .value_of("url")
        .unwrap_or("https://solana-api.projectserum.com");
    let program_id = pubkey_of(&matches, "program_id").unwrap();
    let market = pubkey_of(&matches, "market").expect("Invalid market Pubkey");
    let fee_payer = keypair_of(&matches, FEE_PAYER_ARG.name).unwrap();
    let num_threads = value_t_or_exit!(matches.value_of("threads"), usize);
    let context = Context {
        market,
        fee_payer,
        endpoint: String::from(endpoint),
        program_id,
        num_threads,
    };
    match matches.subcommand() {
        ("liquidate", _) => context.crank_liquidation(),
        ("funding", _) => context.crank_funding(),
        ("garbage-collect", _) => context.garbage_collect(),
        ("funding-extraction", m) => {
            let swarm_size = m
                .unwrap()
                .value_of("swarm_size")
                .unwrap()
                .parse::<u16>()
                .unwrap();
            let node_id = m
                .unwrap()
                .value_of("node_id")
                .unwrap()
                .parse::<u8>()
                .unwrap();
            context.crank_funding_extraction(swarm_size, node_id);
        }
        _ => panic!("Invalid subcommand"),
    }
}
