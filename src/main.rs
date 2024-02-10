use eo_listener::{
    get_blob_index_settled_topic, get_bridge_event_topic, EoAddress, EoServer, EoServerBuilder,
    EoServerError,
};
use std::collections::BTreeSet;
use std::str::FromStr;
use web3::{transports::Http, types::BlockNumber, Web3};

#[tokio::main]
async fn main() -> Result<(), EoServerError> {
    // Initialize Web3 Transport
    //https://u0bddpyrvy:wUYnFGT8qmldV1KsTPoEwo04cVjvVbBFE26pl5rZ_c8@u0v4deab9j-u0ghk9j0sc-rpc.us0-aws.kaleido.io/
    let http: Http =
        Http::new("https://u0bddpyrvy:wUYnFGT8qmldV1KsTPoEwo04cVjvVbBFE26pl5rZ_c8@u0v4deab9j-u0ghk9j0sc-rpc.us0-aws.kaleido.io").map_err(|err| EoServerError::Other(err.to_string()))?;

    // Initialize the ExecutableOracle Address
    // let eo_address = EoAddress::new("0x610178dA211FEF7D417bC0e6FeD39F05609AD788");
    // Initialize the web3 instance
    let web3: Web3<Http> = Web3::new(http);

    let path = "./blocks_processed.dat";
    let eo_server = setup_eo_server(web3, path)?;

    let res = eo_server.run().await;
    println!("{:?}", &res);

    Ok(())
}

fn setup_eo_server(
    web3_instance: web3::Web3<web3::transports::Http>,
    path: &str,
) -> Result<EoServer, EoServerError> {
    // Initialize the ExecutableOracle Address
    let eo_address_str = std::env::var("EO_CONTRACT_ADDRESS").expect("EO_CONTRACT_ADDRESS environment variable is not set. Please set the EO_CONTRACT_ADDRESS environment variable with the Executable Oracle contract address.");
    let eo_address = eo_listener::EoAddress::new(&eo_address_str);
    let contract_address = eo_address
        .parse()
        .map_err(|err| EoServerError::Other(err.to_string()))?;
    let contract_abi = eo_listener::get_abi()?;
    let address = web3::types::Address::from(contract_address);
    let contract = web3::contract::Contract::new(web3_instance.eth(), address, contract_abi);

    let blob_settled_topic = eo_listener::get_blob_index_settled_topic();
    let bridge_topic = eo_listener::get_bridge_event_topic();

    let blob_settled_filter = web3::types::FilterBuilder::default()
        .from_block(BlockNumber::Number(0.into()))
        .to_block(BlockNumber::Latest)
        .address(vec![contract_address])
        .topics(blob_settled_topic.clone(), None, None, None)
        .build();

    let bridge_filter = web3::types::FilterBuilder::default()
        .from_block(BlockNumber::Number(0.into()))
        .to_block(BlockNumber::Latest)
        .address(vec![contract_address])
        .topics(bridge_topic.clone(), None, None, None)
        .build();

    let blob_settled_event = contract
        .abi()
        .event("BlobIndexSettled")
        .map_err(|e| EoServerError::Other(e.to_string()))?
        .clone();

    let bridge_event = contract
        .abi()
        .event("Bridge")
        .map_err(|e| EoServerError::Other(e.to_string()))?
        .clone();

    let eo_server = eo_listener::EoServerBuilder::default()
        .web3(web3_instance)
        .eo_address(eo_address)
        .bridge_processed_blocks(BTreeSet::new())
        .settled_processed_blocks(BTreeSet::new())
        .contract(contract)
        .bridge_topic(bridge_topic)
        .blob_settled_topic(blob_settled_topic)
        .bridge_filter(bridge_filter)
        .current_bridge_filter_block(0.into())
        .current_blob_settlement_filter_block(0.into())
        .blob_settled_filter(blob_settled_filter)
        .blob_settled_event(blob_settled_event)
        .bridge_event(bridge_event)
        .path(std::path::PathBuf::from_str(path).map_err(|e| EoServerError::Other(e.to_string()))?)
        .build()?;

    Ok(eo_server)
}
