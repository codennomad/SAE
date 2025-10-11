use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::ZeroizeOnDrop;

/// Representa a identidade de um peer com chaves de assinatura Ed25519.
/// Isso permite autenticação mútua e previne ataques MITM.
#[derive(ZeroizeOnDrop)]
pub struct Identity {
    signing_key: SigningKey,
    #[zeroize(skip)]
    verifying_key: VerifyingKey,
}

impl Identity {
    /// Cria uma nova identidade com par de chaves Ed25519 aleatório.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        Self {
            signing_key,
            verifying_key,
        }
    }

    /// Retorna a chave de verificação pública.
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    /// Retorna os bytes da chave pública de verificação.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    /// Assina uma mensagem com a chave privada.
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    /// Calcula o fingerprint da identidade (SHA256 da chave pública).
    pub fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.verifying_key.as_bytes());
        let result = hasher.finalize();
        hex::encode(&result[..16]) // 128 bits para facilitar verificação
    }
}

/// Verifica uma assinatura Ed25519.
pub fn verify_signature(
    verifying_key: &VerifyingKey,
    message: &[u8],
    signature: &Signature,
) -> Result<(), SignatureError> {
    verifying_key
        .verify(message, signature)
        .map_err(|_| SignatureError::InvalidSignature)
}

/// Calcula o fingerprint de uma chave pública.
pub fn get_fingerprint(verifying_key: &VerifyingKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifying_key.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..16])
}

/// Estrutura para o handshake inicial com autenticação.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedHandshake {
    /// Chave pública X25519 para ECDH (como Vec para compatibilidade com serde)
    pub x25519_public_key: Vec<u8>,
    /// Chave pública Ed25519 para verificação de assinatura
    pub ed25519_public_key: Vec<u8>,
    /// Assinatura da chave X25519 com a chave Ed25519
    pub signature: Vec<u8>,
}

impl AuthenticatedHandshake {
    /// Cria um novo handshake autenticado.
    pub fn new(x25519_key: [u8; 32], identity: &Identity) -> Self {
        let signature = identity.sign(&x25519_key);

        Self {
            x25519_public_key: x25519_key.to_vec(),
            ed25519_public_key: identity.public_key_bytes().to_vec(),
            signature: signature.to_bytes().to_vec(),
        }
    }

    /// Verifica a autenticidade do handshake.
    pub fn verify(&self) -> Result<VerifyingKey, SignatureError> {
        if self.ed25519_public_key.len() != 32 {
            return Err(SignatureError::InvalidPublicKey);
        }
        if self.signature.len() != 64 {
            return Err(SignatureError::InvalidSignature);
        }
        if self.x25519_public_key.len() != 32 {
            return Err(SignatureError::InvalidPublicKey);
        }

        let ed_key_bytes: [u8; 32] = self.ed25519_public_key[..].try_into()
            .map_err(|_| SignatureError::InvalidPublicKey)?;
        let verifying_key = VerifyingKey::from_bytes(&ed_key_bytes)
            .map_err(|_| SignatureError::InvalidPublicKey)?;

        let sig_bytes: [u8; 64] = self.signature[..].try_into()
            .map_err(|_| SignatureError::InvalidSignature)?;
        let signature = Signature::from_bytes(&sig_bytes);

        verify_signature(&verifying_key, &self.x25519_public_key, &signature)?;

        Ok(verifying_key)
    }

    /// Retorna o fingerprint da identidade do peer.
    pub fn fingerprint(&self) -> Result<String, SignatureError> {
        if self.ed25519_public_key.len() != 32 {
            return Err(SignatureError::InvalidPublicKey);
        }
        let ed_key_bytes: [u8; 32] = self.ed25519_public_key[..].try_into()
            .map_err(|_| SignatureError::InvalidPublicKey)?;
        let verifying_key = VerifyingKey::from_bytes(&ed_key_bytes)
            .map_err(|_| SignatureError::InvalidPublicKey)?;
        Ok(get_fingerprint(&verifying_key))
    }

    /// Retorna a chave X25519 como array.
    pub fn x25519_key_array(&self) -> Result<[u8; 32], SignatureError> {
        self.x25519_public_key[..].try_into()
            .map_err(|_| SignatureError::InvalidPublicKey)
    }

    /// Retorna a chave Ed25519 como array.
    pub fn ed25519_key_array(&self) -> Result<[u8; 32], SignatureError> {
        self.ed25519_public_key[..].try_into()
            .map_err(|_| SignatureError::InvalidPublicKey)
    }
}

/// Erros relacionados a assinaturas digitais.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureError {
    InvalidSignature,
    InvalidPublicKey,
}

impl std::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignatureError::InvalidSignature => write!(f, "Assinatura inválida - possível ataque MITM"),
            SignatureError::InvalidPublicKey => write!(f, "Chave pública inválida"),
        }
    }
}

impl std::error::Error for SignatureError {}
