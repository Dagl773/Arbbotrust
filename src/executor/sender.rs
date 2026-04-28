//! Transaction signer + sender. Wraps an alloy provider and a private-key
//! signer; honours `MAX_GAS_PRICE_GWEI` and produces a tx hash.

use alloy::network::EthereumWallet;
use alloy::primitives::{Address, B256, Bytes, U256};
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::local::PrivateKeySigner;
use anyhow::{Context as _, Result, anyhow, bail};

#[derive(Debug, Clone)]
pub struct SendRequest {
    pub to: Address,
    pub data: Bytes,
    pub value: U256,
    pub gas_limit: u64,
    /// Cap on `maxFeePerGas` (wei). CLAUDE.md §9.5.
    pub max_fee_per_gas_wei: u128,
    pub max_priority_fee_per_gas_wei: u128,
}

#[derive(Debug, Clone)]
pub struct SendOutcome {
    pub tx_hash: B256,
    pub block_number: Option<u64>,
}

/// Live transaction sender backed by an alloy provider + local signer.
pub struct LiveSender {
    provider: DynProvider,
    signer_address: Address,
    max_gas_price_wei: u128,
}

impl LiveSender {
    pub async fn new(
        http_url: &str,
        private_key_hex: &str,
        max_gas_price_gwei: u64,
    ) -> Result<Self> {
        let signer: PrivateKeySigner = private_key_hex
            .parse()
            .context("parse EXECUTOR_PRIVATE_KEY")?;
        let signer_address = signer.address();
        let wallet = EthereumWallet::from(signer);
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect(http_url)
            .await
            .context("connect HTTP provider for sender")?
            .erased();
        Ok(Self {
            provider,
            signer_address,
            max_gas_price_wei: u128::from(max_gas_price_gwei) * 1_000_000_000,
        })
    }

    pub fn signer_address(&self) -> Address {
        self.signer_address
    }

    /// Submit a signed EIP-1559 transaction. Refuses to send if
    /// `max_fee_per_gas_wei` exceeds the configured cap.
    pub async fn send(&self, request: SendRequest) -> Result<SendOutcome> {
        if request.max_fee_per_gas_wei > self.max_gas_price_wei {
            bail!(
                "refusing to send: maxFeePerGas {} > MAX_GAS_PRICE {}",
                request.max_fee_per_gas_wei,
                self.max_gas_price_wei
            );
        }
        let tx = TransactionRequest::default()
            .from(self.signer_address)
            .to(request.to)
            .input(request.data.into())
            .value(request.value)
            .gas_limit(request.gas_limit)
            .max_fee_per_gas(request.max_fee_per_gas_wei)
            .max_priority_fee_per_gas(request.max_priority_fee_per_gas_wei);

        let pending = self
            .provider
            .send_transaction(tx)
            .await
            .map_err(|e| anyhow!("send_transaction: {e}"))?;
        let tx_hash = *pending.tx_hash();
        let receipt = pending
            .with_required_confirmations(1)
            .get_receipt()
            .await
            .map_err(|e| anyhow!("await receipt: {e}"))?;

        Ok(SendOutcome {
            tx_hash,
            block_number: receipt.block_number,
        })
    }
}
