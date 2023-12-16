#![allow(unused)]
use derive_builder::Builder;
use tokio::sync::{oneshot::Receiver, mpsc::UnboundedSender};
use tokio::sync::mpsc::UnboundedReceiver;
use serde::{Serialize, Deserialize};
use web3::types::U64;
use web3::{
    Error as Web3Error,
    Web3, 
    transports::Http, 
    types::{
        FilterBuilder,
        H160,
        H256,
        Log,
        BlockNumber,
        Address, 
        BlockId, U256
    }, contract::{
        Contract,
        tokens::{
            Detokenize,
            Tokenize
        },
        Options
    }, Transport
}; 
use jsonrpsee::{proc_macros::rpc, core::Error};
use sha3::{Digest, Keccak256};

#[macro_export]
macro_rules! log_handler {
    () => {
        |logs| match logs {
            Ok(l) => l,
            Err(_) => Vec::new()
       }
    };
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SettlementLayer {
    Ethereum,
    Other(usize),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    Log(Log),
    Tx {
        content_id: String,
        token_address: Option<String>,
        contract_abi: Option<String>,
        from: String,
        op: String,
        inputs: String,
        settlement_layer: SettlementLayer
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EventType {
    Bridge(web3::ethabi::Event),
    Settlement(web3::ethabi::Event)
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct StopToken;

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum EoServerError {
    Other(String)
}

impl From<EoServerBuilderError> for EoServerError {
    fn from(value: EoServerBuilderError) -> Self {
        Self::Other(value.to_string())
    }
}

impl std::fmt::Display for EoServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractAddress([u8; 32]);

impl From<String> for ContractAddress {
    fn from(value: String) -> Self {
        let arr = value[..64].as_bytes();
        let mut bytes = [0u8; 32];
        for (idx, byte) in arr.iter().enumerate() {
            bytes[idx] = *byte;
        }

        ContractAddress(bytes)
    }
}

/// An ExecutableOracle server that listens for events emitted from 
/// Smart Contracts
#[derive(Builder, Debug, Clone)]
pub struct EoServer<M> {
    web3: Web3<Http>,
    eo_address: EoAddress,
    last_processed_block: BlockNumber,
    parent_actor: Option<ractor::ActorRef<M>>
}

impl<M> EoServer<M> {
    /// The core method of this struct, opens up a listener 
    /// and listens for specific events from the Ethereum Executable 
    /// oracle contract, it then `handles` the events, which is to 
    /// say it schedules tasks to be executed in the Versatus network
    pub async fn run(
        &mut self,
    ) -> web3::Result<()> {
        let contract_address = self.eo_address.parse().map_err(|err| {
            web3::Error::Decoder(err.to_string())
        })?;
        let contract_abi = web3::ethabi::Contract::load(&include_bytes!("../eo_contract_abi.json")[..]).map_err(|e| {
            Web3Error::from(e.to_string())
        })?; 
        let address = web3::types::Address::from(contract_address);
        let contract = Contract::new(self.web3.eth(), address, contract_abi);
        
        let blob_settled_topic = self.get_blob_index_settled_topic();
        let bridge_topic = self.get_bridge_event_topic();

        let blob_settled_filter = FilterBuilder::default()
            .address(vec![contract_address])
            .topics(blob_settled_topic, None, None, None)
            .build();

        let bridge_filter = FilterBuilder::default()
            .address(vec![contract_address])
            .topics(bridge_topic, None, None, None)
            .build();

        let blob_settled_event = contract.abi().event("BlobIndexSettled").map_err(|e| {
           Web3Error::from(e.to_string()) 
        })?.clone();

        let bridge_event = contract.abi().event("Bridge").map_err(|e| {
            Web3Error::from(e.to_string())
        })?.clone();

        loop {
            let log_handler = log_handler!();
            tokio::select!(
                blob_settled_logs = self.web3.eth().logs(blob_settled_filter.clone()) => {
                    self.process_logs(EventType::Settlement(blob_settled_event.clone()), blob_settled_logs, log_handler).await;
                },
                bridge_logs = self.web3.eth().logs(bridge_filter.clone()) => {
                    self.process_logs(EventType::Bridge(bridge_event.clone()), bridge_logs, log_handler).await;
                },
            );
        }        
        Ok(())
    }

    async fn process_logs<F>(
        &mut self,
        event_type: EventType,
        logs: Result<Vec<Log>, Web3Error>,
        handler: F
    ) -> Result<(), EoServerError> 
    where
        F: FnOnce(Result<Vec<Log>, Web3Error>) -> Vec<Log> 
    {
        let events = handler(logs);
        match event_type {
            EventType::Bridge(event_abi) => { self.handle_bridge_event(events, &event_abi)?; },
            EventType::Settlement(event_abi) => { self.handle_settlement_event(events, &event_abi)?; }
        }

        Ok(())
    }

    // TODO(asmith): Handle actual events
    async fn handle_event(&mut self, log: Log) {
        println!("Received log: {:?}", log);
    }

    fn handle_bridge_event(&mut self, events: Vec<Log>, event_abi: &web3::ethabi::Event) -> Result<(), EoServerError> {
        for event in events {
            let block_number = event.block_number.ok_or(EoServerError::Other("Log missing block number".to_string()))?;
            if !self.block_processed(block_number) {
                println!("parsing bridge event");
                self.parse_bridge_event(event, event_abi)?;
                self.last_processed_block = BlockNumber::Number(block_number);
            }
        }
        Ok(())
    }

    fn handle_settlement_event(&mut self, events: Vec<Log>, event_abi: &web3::ethabi::Event) -> Result<(), EoServerError> {
        for event in events {
            let block_number = event.block_number.ok_or(EoServerError::Other("Log missing block number".to_string()))?;
            if !self.block_processed(block_number) {
                println!("parsing bridge event");
                self.parse_settlement_event(event, event_abi)?;
                self.last_processed_block = BlockNumber::Number(block_number);
            }
        }
        Ok(())
    }

    fn parse_bridge_event(&self, event: Log, event_abi: &web3::ethabi::Event) -> Result<(), EoServerError> {
        let parsed_log = event_abi.parse_log(
            web3::ethabi::RawLog { 
                topics: event.topics.clone(), 
                data: event.data.0.clone() 
        }).map_err(|e| EoServerError::Other(e.to_string()))?;

        println!("{:?}", &parsed_log);
        Ok(())
    }

    fn parse_settlement_event(&self, event: Log, event_abi: &web3::ethabi::Event) -> Result<(), EoServerError> {
        let parsed_log = event_abi.parse_log(
            web3::ethabi::RawLog { 
                topics: event.topics.clone(), 
                data: event.data.0.clone() 
        }).map_err(|e| EoServerError::Other(e.to_string()))?;

        println!("{:?}", &parsed_log);
        Ok(())
    }

    fn get_blob_index_settled_topic(&self) -> Option<Vec<H256>> {
        let mut hasher = Keccak256::new();
        let blob_index_settled_sig = b"BlobIndexSettled(address,bytes32,string)";
        hasher.update(blob_index_settled_sig);
        let res: [u8; 32] = hasher.finalize().try_into().ok()?;
        let blob_settled_topic = H256::from(res);
        Some(vec![blob_settled_topic])
    }

    fn get_bridge_event_topic(&self) -> Option<Vec<H256>> {
        let mut hasher = Keccak256::new();
        let bridge_sig = b"Bridge(address,address,uint256,uint256,string)";
        hasher.update(bridge_sig);
        let res: [u8; 32] = hasher.finalize().try_into().ok()?;
        let bridge_topic = H256::from(res);
        Some(vec![bridge_topic])
    }

    fn block_processed(&self, block_number: U64) -> bool {
        match self.last_processed_block {
            BlockNumber::Number(bn) => {
                return block_number <= bn
            }
            _ => { 
                return false
            }
        }
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
    pub fn new(address: &str) -> Self {
        EoAddress(address.to_string())
    }

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
