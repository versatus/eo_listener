use std::collections::BTreeSet;

use eo_listener::{
    EoServerError,
    EoServerBuilder,
    EoAddress,
    get_bridge_event_topic,
    get_blob_index_settled_topic
};
use web3::{Web3, transports::Http, types::FilterBuilder};


#[tokio::main]
async fn main() -> Result<(), EoServerError> {
    
    simple_logger::init_with_level(log::Level::Info).map_err(|e| EoServerError::Other(e.to_string()))?;
    // Initialize Web3 Transport
    let http: Http = Http::new("http://127.0.0.1:8545").map_err(|err| {
        EoServerError::Other(err.to_string())
    })?;

    // Initialize the ExecutableOracle Address
    let eo_address = EoAddress::new("0x610178dA211FEF7D417bC0e6FeD39F05609AD788");
    // Initialize the web3 instance
    let web3: Web3<Http> = Web3::new(http);

    let contract_address = eo_address.parse().map_err(|err| {
        EoServerError::Other(err.to_string())
    })?;
    let contract_abi = web3::ethabi::Contract::load(&include_bytes!("../eo_contract_abi.json")[..]).map_err(|e| {
        EoServerError::Other(e.to_string())
    })?; 
    let address = web3::types::Address::from(contract_address);
    let contract = web3::contract::Contract::new(web3.eth(), address, contract_abi);
    
    let blob_settled_topic = get_blob_index_settled_topic();
    let bridge_topic = get_bridge_event_topic();

    let blob_settled_filter = FilterBuilder::default()
        .address(vec![contract_address])
        .topics(blob_settled_topic, None, None, None)
        .build();

    let bridge_filter = FilterBuilder::default()
        .address(vec![contract_address])
        .topics(bridge_topic, None, None, None)
        .build();

    let blob_settled_event = contract.abi().event("BlobIndexSettled").map_err(|e| {
       EoServerError::Other(e.to_string()) 
    })?.clone();

    let bridge_event = contract.abi().event("Bridge").map_err(|e| {
        EoServerError::Other(e.to_string())
    })?.clone();

    
    let mut eo_server = EoServerBuilder::default()
        .web3(web3)
        .eo_address(eo_address)
        .processed_blocks(BTreeSet::new())
        .contract(contract)
        .bridge_filter(bridge_filter)
        .blob_settled_filter(blob_settled_filter)
        .blob_settled_event(blob_settled_event)
        .bridge_event(bridge_event)
        .build()?;

    let res = eo_server.run().await;
    println!("{:?}", &res);
    
    Ok(())
}
