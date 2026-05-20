// src/startup_sentinel.rs

#[cfg(feature = "tpm")]
pub use tpm_impl::*;

#[cfg(feature = "tpm")]
mod tpm_impl {
    use tss_esapi::{Context, TctiNameConf};
    use std::str::FromStr;

    #[derive(Debug)]
    pub enum SentinelError {
        TctiParse(String),
        ContextInit(String),
        GetRandom(String),
        InsufficientEntropy,
    }

    impl std::fmt::Display for SentinelError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                SentinelError::TctiParse(s)    => write!(f, "TCTI_PARSE_ERROR: {}", s),
                SentinelError::ContextInit(s)  => write!(f, "TPM_CONTEXT_INIT_ERROR: {}", s),
                SentinelError::GetRandom(s)    => write!(f, "TPM_GET_RANDOM_ERROR: {}", s),
                SentinelError::InsufficientEntropy => write!(f, "TPM_INSUFFICIENT_ENTROPY"),
            }
        }
    }

    pub struct StartupSentinel {
        tcti_conf: TctiNameConf,
    }

    impl StartupSentinel {
        pub fn new(tcti_str: &str) -> Result<Self, SentinelError> {
            let tcti_conf = TctiNameConf::from_str(tcti_str)
                .map_err(|e| SentinelError::TctiParse(e.to_string()))?;
            Ok(Self { tcti_conf })
        }

        // Reads TSS2_TCTI → TPM2TOOLS_TCTI → falls back to /dev/tpm0.
        pub fn from_env() -> Result<Self, SentinelError> {
            let tcti_str = std::env::var("TSS2_TCTI")
                .or_else(|_| std::env::var("TPM2TOOLS_TCTI"))
                .unwrap_or_else(|_| "device:/dev/tpm0".to_string());
            Self::new(&tcti_str)
        }

        // Returns 8 bytes of TPM-generated entropy, or fails closed.
        pub fn attest_startup(&self) -> Result<[u8; 8], SentinelError> {
            let mut ctx = Context::new(self.tcti_conf.clone())
                .map_err(|e| SentinelError::ContextInit(e.to_string()))?;

            let digest = ctx.get_random(8)
                .map_err(|e| SentinelError::GetRandom(e.to_string()))?;

            let bytes = digest.value();
            if bytes.len() < 8 {
                return Err(SentinelError::InsufficientEntropy);
            }
            let mut out = [0u8; 8];
            out.copy_from_slice(&bytes[..8]);
            Ok(out)
        }
    }
}
