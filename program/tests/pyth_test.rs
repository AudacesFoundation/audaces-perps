#![cfg(not(feature = "test-bpf"))]

use pyth_client::{CorpAction, PriceStatus, PriceType};

#[test]
fn test_pyth_oracle() {
    use audaces_protocol::{processor::PYTH_MAPPING_ACCOUNT, utils::get_attr_str};
    use pyth_client::{cast, Mapping, Price, Product, PROD_HDR_SIZE};
    use solana_client::rpc_client::RpcClient;
    use solana_program::pubkey::Pubkey;
    use std::str::FromStr;

    let devnet_url = "http://api.devnet.solana.com";
    let rpc_client = RpcClient::new(devnet_url.to_string());
    let mut pyth_mapping_account = Pubkey::from_str(PYTH_MAPPING_ACCOUNT).unwrap();

    loop {
        // Get Mapping account from key
        let map_data = rpc_client.get_account_data(&pyth_mapping_account).unwrap();
        let map_acct = cast::<Mapping>(&map_data);

        // Get and print each Product in Mapping directory
        let mut i = 0;
        for prod_akey in &map_acct.products {
            let prod_pkey = Pubkey::new(&prod_akey.val);
            let prod_data = rpc_client.get_account_data(&prod_pkey).unwrap();
            let prod_acct = cast::<Product>(&prod_data);

            // print key and reference data for this Product
            println!("product_account .. {:?}", prod_pkey);
            let mut psz = prod_acct.size as usize - PROD_HDR_SIZE;
            let mut pit = (&prod_acct.attr[..]).iter();
            while psz > 0 {
                let key = get_attr_str(&mut pit);
                let val = get_attr_str(&mut pit);
                println!("  {:.<16} {}", key, val);
                psz -= 2 + key.len() + val.len();
            }

            // print all Prices that correspond to this Product
            if prod_acct.px_acc.is_valid() {
                let mut px_pkey = Pubkey::new(&prod_acct.px_acc.val);
                loop {
                    let pd = rpc_client.get_account_data(&px_pkey).unwrap();
                    let pa = cast::<Price>(&pd);
                    println!("  price_account .. {:?}", px_pkey);
                    println!("    price_type ... {}", get_price_type(&pa.ptype));
                    println!("    exponent ..... {}", pa.expo);
                    println!("    status ....... {}", get_status(&pa.agg.status));
                    println!("    corp_act ..... {}", get_corp_act(&pa.agg.corp_act));
                    println!("    price ........ {}", pa.agg.price);
                    println!("    conf ......... {}", pa.agg.conf);
                    println!("    valid_slot ... {}", pa.valid_slot);
                    println!("    publish_slot . {}", pa.agg.pub_slot);

                    // go to next price account in list
                    if pa.next.is_valid() {
                        px_pkey = Pubkey::new(&pa.next.val);
                    } else {
                        break;
                    }
                }
            }
            // go to next product
            i += 1;
            if i == map_acct.num {
                break;
            }
        }

        // go to next Mapping account in list
        if !map_acct.next.is_valid() {
            break;
        }
        pyth_mapping_account = Pubkey::new(&map_acct.next.val);
    }
}

//Utils

pub fn get_price_type(ptype: &PriceType) -> &'static str {
    match ptype {
        PriceType::Unknown => "unknown",
        PriceType::Price => "price",
        // PriceType::TWAP => "twap",
        // PriceType::Volatility => "volatility",
    }
}

pub fn get_status(st: &PriceStatus) -> &'static str {
    match st {
        PriceStatus::Unknown => "unknown",
        PriceStatus::Trading => "trading",
        PriceStatus::Halted => "halted",
        PriceStatus::Auction => "auction",
    }
}

pub fn get_corp_act(cact: &CorpAction) -> &'static str {
    match cact {
        CorpAction::NoCorpAct => "nocorpact",
    }
}
