use chacha20poly1305::{aead::{Aead, KeyInit}, ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};
use std::time::{SystemTime, UNIX_EPOCH};

const HKDF_INFO_SEND: &[u8] = b"sae-ratchet-send";
const HKDF_INFO_RECV: &[u8] = b"sae-ratchet-recv";
const MAX_SKIP: usize = 100; // Máximo de mensagens puladas antes de rejeitar

/// Double Ratchet implementação simplificada para Perfect Forward Secrecy
#[derive(ZeroizeOnDrop)]
pub struct RatchetSession {
    /// Chave de cadeia de envio
    send_chain_key: [u8; 32],
    /// Chave de cadeia de recebimento
    recv_chain_key: [u8; 32],
    /// Contador de mensagens enviadas
    send_count: u64,
    /// Contador de mensagens recebidas
    recv_count: u64,
    /// Cache de chaves puladas para mensagens fora de ordem
    #[zeroize(skip)]
    skipped_keys: Vec<([u8; 32], u64)>,
}

impl RatchetSession {
    /// Cria uma nova sessão de ratchet a partir de um segredo compartilhado
    pub fn new(shared_secret: &[u8; 32]) -> Self {
        // Deriva chaves de cadeia iniciais separadas para cada direção
        let hkdf_send = Hkdf::<Sha256>::new(None, shared_secret);
        let mut send_chain_key = [0u8; 32];
        hkdf_send.expand(HKDF_INFO_SEND, &mut send_chain_key)
            .expect("HKDF expand failed");

        let hkdf_recv = Hkdf::<Sha256>::new(None, shared_secret);
        let mut recv_chain_key = [0u8; 32];
        hkdf_recv.expand(HKDF_INFO_RECV, &mut recv_chain_key)
            .expect("HKDF expand failed");

        Self {
            send_chain_key,
            recv_chain_key,
            send_count: 0,
            recv_count: 0,
            skipped_keys: Vec::new(),
        }
    }

    /// Criptografa uma mensagem e avança o ratchet de envio
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<RatchetMessage, RatchetError> {
        // Deriva chave de mensagem da chave de cadeia
        let (message_key, next_chain_key) = self.derive_key(&self.send_chain_key);

        // Criptografa com ChaCha20-Poly1305
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&message_key));
        let nonce = self.generate_nonce(self.send_count);

        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| RatchetError::EncryptionFailed)?;

        // Avança o ratchet
        let msg = RatchetMessage {
            counter: self.send_count,
            ciphertext,
            timestamp: Self::current_timestamp(),
        };

        self.send_chain_key = next_chain_key;
        self.send_count += 1;

        Ok(msg)
    }

    /// Descriptografa uma mensagem e avança o ratchet de recebimento
    pub fn decrypt(&mut self, message: &RatchetMessage) -> Result<Vec<u8>, RatchetError> {
        // Verifica timestamp para detectar replays
        let current_time = Self::current_timestamp();
        if message.timestamp > current_time + 60 {
            // Mensagem do futuro
            return Err(RatchetError::InvalidTimestamp);
        }
        if current_time - message.timestamp > 300 {
            // Mensagem muito antiga (> 5 minutos)
            return Err(RatchetError::MessageTooOld);
        }

        // Verifica se a mensagem está na ordem esperada
        if message.counter == self.recv_count {
            // Mensagem em ordem
            let (message_key, next_chain_key) = self.derive_key(&self.recv_chain_key);
            let plaintext = self.decrypt_with_key(&message_key, message)?;

            self.recv_chain_key = next_chain_key;
            self.recv_count += 1;

            Ok(plaintext)
        } else if message.counter > self.recv_count {
            // Mensagem fora de ordem - armazena chaves puladas
            let skip_count = (message.counter - self.recv_count) as usize;

            if skip_count > MAX_SKIP {
                return Err(RatchetError::TooManySkippedMessages);
            }

            // Deriva e armazena chaves para mensagens puladas
            let mut chain_key = self.recv_chain_key;
            for i in 0..skip_count {
                let (msg_key, next_key) = self.derive_key(&chain_key);
                self.skipped_keys.push((msg_key, self.recv_count + i as u64));
                chain_key = next_key;
            }

            // Descriptografa a mensagem atual
            let (message_key, next_chain_key) = self.derive_key(&chain_key);
            let plaintext = self.decrypt_with_key(&message_key, message)?;

            self.recv_chain_key = next_chain_key;
            self.recv_count = message.counter + 1;

            Ok(plaintext)
        } else {
            // Mensagem antiga - verifica se temos a chave armazenada
            if let Some(pos) = self.skipped_keys.iter().position(|(_, count)| *count == message.counter) {
                let (message_key, _) = self.skipped_keys.remove(pos);
                self.decrypt_with_key(&message_key, message)
            } else {
                Err(RatchetError::MessageAlreadyReceived)
            }
        }
    }

    /// Deriva uma chave de mensagem e a próxima chave de cadeia usando HKDF
    fn derive_key(&self, chain_key: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
        let hkdf = Hkdf::<Sha256>::new(None, chain_key);
        let mut output = [0u8; 64]; // 32 bytes para message key + 32 para next chain key
        hkdf.expand(b"sae-ratchet-kdf", &mut output)
            .expect("HKDF expand failed");

        let mut message_key = [0u8; 32];
        let mut next_chain_key = [0u8; 32];
        message_key.copy_from_slice(&output[..32]);
        next_chain_key.copy_from_slice(&output[32..]);

        (message_key, next_chain_key)
    }

    /// Descriptografa com uma chave específica
    fn decrypt_with_key(&self, key: &[u8; 32], message: &RatchetMessage) -> Result<Vec<u8>, RatchetError> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let nonce = self.generate_nonce(message.counter);

        cipher
            .decrypt(&nonce, message.ciphertext.as_slice())
            .map_err(|_| RatchetError::DecryptionFailed)
    }

    /// Gera um nonce único baseado no contador
    fn generate_nonce(&self, counter: u64) -> Nonce {
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..12].copy_from_slice(&counter.to_le_bytes());
        nonce_bytes.into()
    }

    /// Retorna o timestamp Unix atual
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

/// Mensagem criptografada pelo ratchet
#[derive(Debug, Clone)]
pub struct RatchetMessage {
    /// Contador da mensagem para ordenação
    pub counter: u64,
    /// Dados criptografados
    pub ciphertext: Vec<u8>,
    /// Timestamp Unix para proteção contra replay
    pub timestamp: u64,
}

impl RatchetMessage {
    /// Serializa a mensagem para bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.counter.to_le_bytes());
        bytes.extend_from_slice(&self.timestamp.to_le_bytes());
        bytes.extend_from_slice(&(self.ciphertext.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    /// Deserializa bytes para uma mensagem
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RatchetError> {
        if bytes.len() < 20 {
            // 8 (counter) + 8 (timestamp) + 4 (length)
            return Err(RatchetError::InvalidMessage);
        }

        let counter = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let timestamp = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let length = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;

        if bytes.len() < 20 + length {
            return Err(RatchetError::InvalidMessage);
        }

        let ciphertext = bytes[20..20 + length].to_vec();

        Ok(Self {
            counter,
            ciphertext,
            timestamp,
        })
    }
}

/// Erros do ratchet
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatchetError {
    EncryptionFailed,
    DecryptionFailed,
    InvalidMessage,
    InvalidTimestamp,
    MessageTooOld,
    MessageAlreadyReceived,
    TooManySkippedMessages,
}

impl std::fmt::Display for RatchetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RatchetError::EncryptionFailed => write!(f, "Falha na criptografia"),
            RatchetError::DecryptionFailed => write!(f, "Falha na descriptografia - mensagem corrompida ou chave incorreta"),
            RatchetError::InvalidMessage => write!(f, "Formato de mensagem inválido"),
            RatchetError::InvalidTimestamp => write!(f, "Timestamp inválido - possível ataque"),
            RatchetError::MessageTooOld => write!(f, "Mensagem muito antiga - possível replay attack"),
            RatchetError::MessageAlreadyReceived => write!(f, "Mensagem já foi recebida - replay attack detectado"),
            RatchetError::TooManySkippedMessages => write!(f, "Muitas mensagens puladas - possível ataque"),
        }
    }
}

impl std::error::Error for RatchetError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ratchet_basic() {
        let secret = [42u8; 32];
        let mut alice = RatchetSession::new(&secret);
        let mut bob = RatchetSession::new(&secret);

        // Alice envia mensagem
        let msg1 = alice.encrypt(b"Hello Bob!").unwrap();
        let decrypted1 = bob.decrypt(&msg1).unwrap();
        assert_eq!(decrypted1, b"Hello Bob!");

        // Bob responde
        let msg2 = bob.encrypt(b"Hello Alice!").unwrap();
        let decrypted2 = alice.decrypt(&msg2).unwrap();
        assert_eq!(decrypted2, b"Hello Alice!");
    }

    #[test]
    fn test_ratchet_forward_secrecy() {
        let secret = [42u8; 32];
        let mut alice = RatchetSession::new(&secret);
        let mut bob = RatchetSession::new(&secret);

        let msg1 = alice.encrypt(b"Message 1").unwrap();
        let msg2 = alice.encrypt(b"Message 2").unwrap();

        // Bob recebe mensagens em ordem
        bob.decrypt(&msg1).unwrap();
        bob.decrypt(&msg2).unwrap();

        // Tentar descriptografar msg1 novamente deve falhar (forward secrecy)
        assert!(bob.decrypt(&msg1).is_err());
    }
}
