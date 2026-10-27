//! Tokens de agente (D09): se entregan una vez y se guardan solo como hash.

use sha2::{Digest, Sha256};

/// Prefijo visible de los tokens, para reconocerlos en una configuración.
pub const TOKEN_PREFIX: &str = "tc_";

/// Token nuevo: `tc_` + 32 bytes aleatorios en hexadecimal. `None` si el
/// sistema no da aleatoriedad (no se emite un token débil).
pub fn generate_token() -> Option<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).ok()?;
    Some(format!("{TOKEN_PREFIX}{}", hex::encode(bytes)))
}

/// SHA-256 del token en hexadecimal: lo único que se guarda.
pub fn hash_token(token: &str) -> String {
    sha256_hex(token.trim().as_bytes())
}

/// SHA-256 de unos datos en hexadecimal (contenido de adjuntos, RF-MOD-17).
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// HMAC-SHA256 (RFC 2104) en hexadecimal: firma de los webhooks (RF-INT-02).
pub fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    const BLOCK: usize = 64;
    // Una clave más larga que el bloque se resume primero; luego se rellena con ceros.
    let digest;
    let key = if key.len() > BLOCK {
        digest = Sha256::digest(key);
        digest.as_slice()
    } else {
        key
    };
    let mut block = [0u8; BLOCK];
    for (b, k) in block.iter_mut().zip(key) {
        *b = *k;
    }
    let pad = |byte: u8| block.map(|b| b ^ byte);
    let inner = Sha256::new()
        .chain_update(pad(0x36))
        .chain_update(message)
        .finalize();
    hex::encode(
        Sha256::new()
            .chain_update(pad(0x5c))
            .chain_update(inner)
            .finalize(),
    )
}

/// Secreto para firmar: `whsec_` + 32 bytes aleatorios en hexadecimal.
pub fn generate_secret() -> Option<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).ok()?;
    Some(format!("whsec_{}", hex::encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vectores de la RFC 4231 (casos 1 y 6).
    #[test]
    fn hmac_matches_rfc_4231() {
        assert_eq!(
            hmac_sha256_hex(&[0x0b; 20], b"Hi There"),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hmac_sha256_hex(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            ),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn tokens_are_unique_and_hash_is_stable() {
        let a = generate_token().unwrap();
        let b = generate_token().unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with(TOKEN_PREFIX));
        assert_eq!(a.len(), TOKEN_PREFIX.len() + 64);
        assert_eq!(hash_token(&a), hash_token(&format!(" {a}\n")));
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
