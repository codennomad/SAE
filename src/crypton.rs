use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};
use zeroize::ZeroizeOnDrop;

const HKDF_INFO: &[u8] = b"sae-hkdf-info";

/// Representa o estado criptográfico de uma sessão segura.
/// Deriva uma chave de criptografia e um sal de nonce usando HKDF.
#[derive(ZeroizeOnDrop)]
pub struct CryptoSession {
    cipher: ChaCha20Poly1305,
    #[zeroize(skip)]
    nonce_counter: u64,
    nonce_salt: [u8; 12],
}

impl CryptoSession {
    /// Cria uma nova sessão criptográfica a partir de um segredo compartilhado.
    pub fn new(shared_secret: &SharedSecret) -> Self {
        let hkdf = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());
        let mut okm = [0u8; 44]; // 32 bytes para a chave + 12 bytes para o sal do nonce
        hkdf.expand(HKDF_INFO, &mut okm)
            .expect("HKDF expand failed");

        let (key_bytes, nonce_salt_bytes) = okm.split_at(32);
        
        let key = Key::from_slice(key_bytes);
        let cipher = ChaCha20Poly1305::new(key);
        
        let mut nonce_salt = [0u8; 12];
        nonce_salt.copy_from_slice(nonce_salt_bytes);

        Self {
            cipher,
            nonce_counter: 0,
            nonce_salt,
        }
    }

    /// Criptografa uma mensagem. O nonce é anexado ao início do ciphertext.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce = self.next_nonce()?;
        let ciphertext = self.cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| CryptoError::EncryptionFailed)?;

        let mut result = nonce.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// Descriptografa uma mensagem. O nonce é lido do início dos dados.
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

    /// Gera o próximo nonce combinando o sal da sessão com um contador.
    fn next_nonce(&mut self) -> Result<Nonce, CryptoError> {
        let mut nonce_bytes = self.nonce_salt; // Começa com o sal
        let counter_bytes = self.nonce_counter.to_le_bytes();

        // XOR do contador nos últimos 8 bytes do sal para criar um nonce único
        for i in 0..8 {
            nonce_bytes[i + 4] ^= counter_bytes[i];
        }

        self.nonce_counter = self.nonce_counter.checked_add(1)
            .ok_or(CryptoError::NonceExhausted)?;
        
        Ok(nonce_bytes.into())
    }
}

/// Gera um par de chaves efêmero X25519.
pub fn generate_keypair() -> (EphemeralSecret, PublicKey) {
    let secret = EphemeralSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    (secret, public)
}

/// Calcula o fingerprint de uma chave pública para verificação fora da banda.
pub fn get_fingerprint(pubkey: &PublicKey) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(pubkey.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..16]) // Retorna os primeiros 128 bits para facilitar a leitura
}

/// Tipos de erro criptográfico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            CryptoError::DecryptionFailed => write!(f, "Decryption failed: authentication tag mismatch"),
            CryptoError::InvalidData => write!(f, "Invalid cryptographic data format"),
            CryptoError::NonceExhausted => write!(f, "Nonce counter exhausted for this session"),
        }
    }
}

impl std::error::Error for CryptoError {}

