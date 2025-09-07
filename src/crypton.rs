use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use chacha20poly1305::aead::{Aead, KeyInit};
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};
use rand::rngs::OsRng;

/// Representa o estado criptográfico de uma sessão
#[derive(ZeroizeOnDrop)]
pub struct CryptoSession {
    cipher: ChaCha20Poly1305,
    #[zeroize(skip)]
    nonce_counter: u64,
}

impl CryptoSession {
    /// Cria uma nova sessão cripto a partir de um segredo compartilhado
    pub fn new(shared_secret: &SharedSecret) -> Self {
        let key = Key::from_slice(shared_secret.as_bytes());
        let cipher = ChaCha20Poly1305::new(key);
        
        Self {
            cipher,
            nonce_counter: 0,
        }
    }
    
    /// Criptografa uma mensagem
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce = self.next_nonce()?;
        let ciphertext = self.cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| CryptoError::EncryptionFailed)?;
        
        // Anexa nonce ao ciphertext
        let mut result = nonce.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }
    
    /// Descriptografa uma mensagem
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if data.len() < 12 {
            return Err(CryptoError::InvalidData);
        }
        
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CryptoError::DecryptionFailed)
    }
    
    /// Gera o próximo nonce
    fn next_nonce(&mut self) -> Result<Nonce, CryptoError> {
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..12].copy_from_slice(&self.nonce_counter.to_le_bytes());
        
        self.nonce_counter += 1;
        if self.nonce_counter == 0 {
            return Err(CryptoError::NonceExhausted);
        }
        
        Ok(*Nonce::from_slice(&nonce_bytes))
    }
}

/// Executa troca de chaves X25519
pub fn perform_key_exchange() -> (EphemeralSecret, PublicKey) {
    let secret = EphemeralSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    (secret, public)
}

/// Completa a troca de chaves e deriva o segredo compartilhado
pub fn complete_key_exchange(
    our_secret: EphemeralSecret,
    their_public: &PublicKey,
) -> SharedSecret {
    our_secret.diffie_hellman(their_public)
}

/// Gera um token aleatório seguro
pub fn generate_token() -> Vec<u8> {
    use rand::RngCore;
    let mut token = vec![0u8; 32];
    OsRng.fill_bytes(&mut token);
    token
}

/// Tipos de erro criptográfico
#[derive(Debug)]
pub enum CryptoError {
    EncryptionFailed,
    DecryptionFailed,
    InvalidData,
    NonceExhausted,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::EncryptionFailed => write!(f, "Encryption failed"),
            CryptoError::DecryptionFailed => write!(f, "Decryption failed"),
            CryptoError::InvalidData => write!(f, "Invalid cryptographic data"),
            CryptoError::NonceExhausted => write!(f, "Nonce counter exhausted"),
        }
    }
}

impl std::error::Error for CryptoError {}