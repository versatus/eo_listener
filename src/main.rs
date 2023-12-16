use eo_server::{EoServerError, EoServerBuilder, EoAddress};
use web3::{Web3, transports::Http, types::{BlockNumber, Log}};


#[tokio::main]
async fn main() -> Result<(), EoServerError> {

    // Initialize Web3 Transport
    let http: Http = Http::new("http://127.0.0.1:8545").map_err(|err| {
        EoServerError::Other(err.to_string())
    })?;

    // Initialize the ExecutableOracle Address
    let eo_address = EoAddress::new("0x610178dA211FEF7D417bC0e6FeD39F05609AD788");
    // Initialize the web3 instance
    let web3: Web3<Http> = Web3::new(http);
    
    let mut eo_server = EoServerBuilder::<Log>::default()
        .web3(web3)
        .eo_address(eo_address)
        .last_processed_block(BlockNumber::Number(web3::types::U64([4])))
        .build()?;

    let res = eo_server.run().await;
    println!("{:?}", &res);
    
    Ok(())
}
