use rand::{Rng, rngs::OsRng};

/// Tamanhos de bloco de padding para ofuscar tamanhos de mensagens
const PADDING_BLOCKS: &[usize] = &[128, 256, 512, 1024, 2048, 4096];

/// Adiciona padding aleatório à mensagem para ofuscar o tamanho real
pub fn add_padding(data: &[u8]) -> Vec<u8> {
    let original_len = data.len();

    // Encontra o próximo tamanho de bloco que acomoda os dados
    let padded_size = PADDING_BLOCKS
        .iter()
        .find(|&&size| size >= original_len + 2) // +2 para armazenar o tamanho original
        .copied()
        .unwrap_or(((original_len + 2 + 4095) / 4096) * 4096); // Arredonda para múltiplo de 4096

    let padding_len = padded_size - original_len - 2;

    // Formato: [tamanho_original: u16][dados][padding_aleatório]
    let mut padded = Vec::with_capacity(padded_size);

    // Armazena o tamanho original (máx 65535 bytes)
    if original_len > u16::MAX as usize {
        panic!("Mensagem muito grande para padding");
    }
    padded.extend_from_slice(&(original_len as u16).to_le_bytes());

    // Adiciona dados originais
    padded.extend_from_slice(data);

    // Adiciona padding aleatório
    let mut rng = OsRng;
    let random_padding: Vec<u8> = (0..padding_len).map(|_| rng.gen()).collect();
    padded.extend_from_slice(&random_padding);

    padded
}

/// Remove o padding e retorna os dados originais
pub fn remove_padding(padded_data: &[u8]) -> Result<Vec<u8>, PaddingError> {
    if padded_data.len() < 2 {
        return Err(PaddingError::InvalidPadding);
    }

    // Lê o tamanho original
    let original_len = u16::from_le_bytes([padded_data[0], padded_data[1]]) as usize;

    if original_len + 2 > padded_data.len() {
        return Err(PaddingError::InvalidPadding);
    }

    // Extrai dados originais
    Ok(padded_data[2..2 + original_len].to_vec())
}

/// Erro de padding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaddingError {
    InvalidPadding,
}

impl std::fmt::Display for PaddingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaddingError::InvalidPadding => write!(f, "Padding inválido"),
        }
    }
}

impl std::error::Error for PaddingError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_padding_roundtrip() {
        let original = b"Hello, World!";
        let padded = add_padding(original);
        let unpadded = remove_padding(&padded).unwrap();

        assert_eq!(original, unpadded.as_slice());
        assert!(padded.len() >= original.len());
    }

    #[test]
    fn test_padding_sizes() {
        // Testa que mensagens de tamanhos similares ficam com o mesmo tamanho após padding
        let msg1 = b"a";
        let msg2 = b"abc";
        let msg3 = b"abcdefghijklmnop";

        let padded1 = add_padding(msg1);
        let padded2 = add_padding(msg2);
        let padded3 = add_padding(msg3);

        // Todas devem ter o mesmo tamanho (128 bytes - o menor bloco)
        assert_eq!(padded1.len(), 128);
        assert_eq!(padded2.len(), 128);
        assert_eq!(padded3.len(), 128);
    }

    #[test]
    fn test_padding_larger_messages() {
        let msg = vec![0u8; 500]; // 500 bytes
        let padded = add_padding(&msg);

        // Deve arredondar para 512 bytes
        assert_eq!(padded.len(), 512);

        let unpadded = remove_padding(&padded).unwrap();
        assert_eq!(unpadded.len(), 500);
    }

    #[test]
    fn test_invalid_padding() {
        let invalid = vec![0xFF, 0xFF, 0, 0]; // Tamanho inválido
        assert!(remove_padding(&invalid).is_err());
    }
}
