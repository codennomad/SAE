use tokio::net::TcpStream;
use tokio_socks::tcp::Socks5Stream;
use std::io;

/// Configuração para conexões via Tor.
pub struct TorConfig {
    /// Endereço do proxy SOCKS5 do Tor (geralmente 127.0.0.1:9050)
    pub socks_addr: String,
    /// Porta do proxy SOCKS5 do Tor
    pub socks_port: u16,
}

impl Default for TorConfig {
    fn default() -> Self {
        Self {
            socks_addr: "127.0.0.1".to_string(),
            socks_port: 9050,
        }
    }
}

impl TorConfig {
    /// Retorna o endereço completo do proxy SOCKS5.
    pub fn proxy_addr(&self) -> String {
        format!("{}:{}", self.socks_addr, self.socks_port)
    }
}

/// Conecta a um host através do Tor usando SOCKS5.
pub async fn connect_via_tor(
    target_host: &str,
    target_port: u16,
    tor_config: &TorConfig,
) -> io::Result<TcpStream> {
    let proxy_addr = tor_config.proxy_addr();

    // Conecta ao proxy Tor SOCKS5
    let stream = Socks5Stream::connect(
        proxy_addr.as_str(),
        (target_host, target_port),
    )
    .await
    .map_err(|e| io::Error::new(io::ErrorKind::ConnectionRefused, e))?;

    Ok(stream.into_inner())
}

/// Verifica se o Tor está rodando testando conexão ao proxy SOCKS5.
pub async fn check_tor_available(tor_config: &TorConfig) -> bool {
    let proxy_addr = tor_config.proxy_addr();

    // Tenta conectar ao proxy Tor
    match TcpStream::connect(proxy_addr).await {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Retorna informações sobre o status do Tor.
pub async fn get_tor_status(tor_config: &TorConfig) -> TorStatus {
    if check_tor_available(tor_config).await {
        TorStatus::Available
    } else {
        TorStatus::Unavailable {
            message: format!(
                "Tor SOCKS5 proxy não está acessível em {}. \
                Certifique-se de que o Tor está rodando:\n\
                - Linux: sudo systemctl start tor\n\
                - macOS/Windows: Execute o Tor Browser ou tor daemon",
                tor_config.proxy_addr()
            ),
        }
    }
}

/// Status da disponibilidade do Tor.
#[derive(Debug, Clone)]
pub enum TorStatus {
    Available,
    Unavailable { message: String },
}

impl TorStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, TorStatus::Available)
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            TorStatus::Available => None,
            TorStatus::Unavailable { message } => Some(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tor_config_default() {
        let config = TorConfig::default();
        assert_eq!(config.proxy_addr(), "127.0.0.1:9050");
    }
}
