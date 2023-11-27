use tokio::sync::oneshot::Receiver;
use web3::{Web3, transports::Http, types::{FilterBuilder, H160, H256, Log, BlockNumber, Address, BlockId}, contract::{Contract, tokens::{Detokenize, Tokenize}, Options}, Transport};

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct StopToken;

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum EoServerError {
    MissingField(String),
    Other(String)
}

#[derive(Debug)]
pub struct EoServerBuilder {
    web3: Option<Web3<Http>>,
    eo_address: Option<EoAddress>,
    event_signature_hash: Option<EventSignatureHash>,
    rx: Option<Receiver<StopToken>>
}

impl EoServerBuilder {
    pub fn new() -> Self {
        Default::default()
    }
    
    pub fn web3(mut self, web3: Web3<Http>) -> Self {
        self.web3 = Some(web3);
        self
    }

    pub fn eo_address(mut self, eo_address: EoAddress) -> Self {
        self.eo_address = Some(eo_address);
        self
    }

    pub fn event_signature_hash(mut self, event_signature_hash: EventSignatureHash) -> Self {
        self.event_signature_hash = Some(event_signature_hash);
        self
    }

    pub fn rx(mut self, rx: Receiver<StopToken>) -> Self {
        self.rx = Some(rx);
        self
    }

    pub fn build(&self) -> Result<EoServer, EoServerError> {
        let web3 = self.web3.clone()
            .ok_or(
                EoServerError::MissingField(
                    "web3 transport filed missing".to_string()
                )
            )?;
        
        let eo_address = self.eo_address.clone()
            .ok_or(
                EoServerError::MissingField(
                    "eo contract address missing".to_string()
                )
            )?;
        
        let event_signature_hash = self.event_signature_hash.clone()
            .ok_or(EoServerError::MissingField("event signature missing".to_string()))?;

        Ok(EoServer {
            web3,
            eo_address,
            event_signature_hash,
        })
    }
}

impl Default for EoServerBuilder {
    fn default() -> Self {
        Self {
            web3: None,
            eo_address: None,
            event_signature_hash: None,
            rx: None
        }
    }
}

/// An ExecutableOracle server that listens for events emitted from 
/// an Ethereum Smart Contract
#[derive(Debug)]
pub struct EoServer {
    web3: Web3<Http>,
    eo_address: EoAddress,
    event_signature_hash: EventSignatureHash,
}

impl EoServer {
    /// The core method of this struct, opens up a listener 
    /// and listens for specific events from the Ethereum Executable 
    /// oracle contract, it then `handles` the events, which is to 
    /// say it schedules tasks to be executed in the Versatus network
    pub async fn run(&mut self, mut rx: Receiver<StopToken>) -> web3::Result<()> {
        let contract_address = self.eo_address.parse().map_err(|err| {
            web3::Error::Decoder(err.to_string())
        })?;
        let topic = self.event_signature_hash.parse().map_err(|err| {
            web3::Error::Decoder(err.to_string())
        })?;

        let filter = FilterBuilder::default()
            .address(vec![contract_address])
            .topics(Some(vec![topic]), None, None, None)
            .build();
        
        loop {
            if let Ok(_) = rx.try_recv() {
                break;
            }
            
            tokio::select! {
                logs = self.web3.eth().logs(filter.clone()) => {
                    match logs {
                        Ok(log) => {
                            for log in log {
                                self.handle_event(log).await;
                            }
                        }
                        Err(_) => {}
                    }
                }
                //TODO(asmith): add ability to request balances and contract data
                //when transactions enter the network
            }
        }        
        Ok(())
    }

    // TODO(asmith): Handle actual events
    async fn handle_event(&mut self, log: Log) {
        println!("Received log: {:?}", log);
    }

    async fn get_account_balance_eth(
        &mut self,
        address: H160,
        block: Option<BlockNumber>
    ) -> Result<web3::types::U256, web3::Error> {
        self.web3.eth().balance(address, block).await
    }

    async fn get_batch_account_balance_eth(
        &mut self,
        addresses: impl IntoIterator<Item = H160>,
        block: Option<BlockNumber>
    ) -> Result<Vec<(H160, web3::types::U256)>, web3::Error> {
        let mut balances = Vec::new();
        for address in addresses {
            let balance = self.get_account_balance_eth(address, block).await?;
            balances.push((address, balance));
        }

        Ok(balances)
    }

    async fn get_account_contract_data<T, R, A, B, P>(
        &mut self,
        address: A,
        contract: Contract<T>,
        block: B,
        function: &str,
        params: P,
        options: Options
    ) -> Result<R, web3::contract::Error> 
    where
        T: Transport,
        R: Detokenize,
        A: Into<Option<Address>>,
        B: Into<Option<BlockId>>,
        P: Tokenize
    {
        contract.query::<R, A, B, P>(function, params, address, options, block).await
    }

    async fn get_batch_account_contract_data<T, R, A, B, P>(
        &mut self,
        account_contract_data: impl IntoIterator<Item = (A, Contract<T>, &str, P, Options)>,
        block: B
    ) -> Result<Vec<(A, R)>, web3::contract::Error> 
    where
        T: Transport,
        R: Detokenize,
        A: Into<Option<Address>> + Clone,
        B: Into<Option<BlockId>> + Clone,
        P: Tokenize
    {
        let mut results = Vec::new();
        for p in account_contract_data {
            let res = self.get_account_contract_data(
                p.0.clone(), p.1, block.clone(), p.2, p.3, p.4
            ).await?;
            results.push((p.0, res));
        }

        Ok(results)
    }
}

#[derive(Clone, Debug)]
pub struct EoAddress(String);

impl EoAddress {
    pub fn parse(&self) -> Result<H160, rustc_hex::FromHexError> {
        self.0.parse()
    }
}

impl EventSignatureHash {
    pub fn parse(&self) -> Result<H256, rustc_hex::FromHexError> {
        self.0.parse()
    }
}

#[derive(Clone, Debug)]
pub struct EventSignatureHash(String);

#[cfg(test)]
mod tests {

}
