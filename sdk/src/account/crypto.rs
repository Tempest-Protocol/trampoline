use crate::account::error::AccountError;
use aes::cipher::{KeyIvInit, StreamCipher};
use anyhow::Result;
use hmac::Hmac;
use pbkdf2::pbkdf2;
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tiny_keccak::Hasher;

// https://github.com/ethereum/wiki/wiki/Web3-Secret-Storage-Definition#pbkdf2-sha-256
const PBKDF2_DKLEN: u32 = 32;
const PBKDF2_C: u32 = 262144;
const KDF_TYPE_PBKDF2: &str = "pbkdf2";

const CIPHER_AES128CTR: &str = "aes-128-ctr";

type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum Prf {
    #[serde(rename = "hmac-sha256")]
    HmacSha256,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Pbkdf2 {
    prf: Prf,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes_32"
    )]
    salt: [u8; 32],
    c: u32,
    dklen: u32,
}

impl Default for Pbkdf2 {
    fn default() -> Self {
        Pbkdf2 {
            prf: Prf::HmacSha256,
            salt: rand::thread_rng().gen(),
            c: PBKDF2_C,
            dklen: PBKDF2_DKLEN,
        }
    }
}

impl Pbkdf2 {
    pub fn new_with_salt(salt: [u8; 32]) -> Self {
        Pbkdf2 {
            salt,
            ..Default::default()
        }
    }
    pub fn pdf_key(&self, password: &[u8]) -> [u8; 32] {
        let mut res = [0u8; 32];
        pbkdf2::<Hmac<sha2::Sha256>>(password, self.salt.as_slice(), self.c, &mut res);
        res
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kdf", content = "kdfparams", rename_all = "lowercase")]
pub enum Kdf {
    Pbkdf2(Pbkdf2),
}

impl Default for Kdf {
    fn default() -> Self {
        Kdf::Pbkdf2(Pbkdf2::default())
    }
}

impl Kdf {
    pub fn kdf_key(&self, password: &[u8]) -> [u8; 32] {
        match self {
            Kdf::Pbkdf2(params) => params.pdf_key(password),
        }
    }

    pub fn kdf_type(&self) -> &'static str {
        match self {
            Kdf::Pbkdf2(_) => KDF_TYPE_PBKDF2,
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cipher", content = "cipherparams")]
pub enum Cipher {
    #[serde(rename = "aes-128-ctr")]
    Aes128Ctr(Aes128CtrParams),
}

impl Default for Cipher {
    fn default() -> Self {
        Cipher::Aes128Ctr(Aes128CtrParams::default())
    }
}

impl Cipher {
    pub fn cipher_type(&self) -> &'static str {
        CIPHER_AES128CTR
    }

    pub fn encrypt_inplace(&self, key: &[u8], data: &mut [u8]) {
        self.apply_inplace(key, data)
    }

    pub fn decrypt_inplace(&self, key: &[u8], data: &mut [u8]) {
        self.apply_inplace(key, data)
    }

    // The code of encryption and decryption of aes-1280-ctr is same.
    fn apply_inplace(&self, key: &[u8], data: &mut [u8]) {
        match self {
            Cipher::Aes128Ctr(p) => {
                assert!(key.len() >= 16);
                let mut cipher = Aes128Ctr::new(key[..16].into(), p.iv.as_slice().into());
                cipher.apply_keystream(data);
            }
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Aes128CtrParams {
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes_16"
    )]
    iv: [u8; 16],
}

impl Default for Aes128CtrParams {
    fn default() -> Self {
        Aes128CtrParams {
            iv: rand::thread_rng().gen(),
        }
    }
}

impl Aes128CtrParams {
    pub fn new(iv: [u8; 16]) -> Self {
        Aes128CtrParams { iv }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Crypto {
    #[serde(flatten)]
    cipher: Cipher,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes"
    )]
    ciphertext: Vec<u8>,
    #[serde(flatten)]
    kdf: Kdf,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes_32"
    )]
    mac: [u8; 32],
}

impl Crypto {
    pub fn encrypt_key(key: &[u8], password: &[u8]) -> Crypto {
        Crypto::encrypt_key_with_kdf_and_cipher(key, password, Kdf::default(), Cipher::default())
    }

    pub fn encrypt_key_with_kdf_and_cipher(
        key: &[u8],
        password: &[u8],
        kdf: Kdf,
        cipher: Cipher,
    ) -> Crypto {
        let kdf_key = kdf.kdf_key(password);
        let mut ciphertext = key.to_vec();
        cipher.encrypt_inplace(&kdf_key, &mut ciphertext);
        let mac = calculate_mac(&ciphertext, &kdf_key);
        Crypto {
            cipher,
            kdf,
            ciphertext,
            mac,
        }
    }

    pub fn decrypt_key(&self, password: &[u8]) -> Result<Vec<u8>, AccountError> {
        let mut plain_text = self.ciphertext.clone();
        let kdf_key = self.kdf.kdf_key(password);
        let mac = calculate_mac(&self.ciphertext, &kdf_key);
        if mac != self.mac {
            return Err(AccountError::WrongPassword);
        }

        self.cipher.decrypt_inplace(&kdf_key, &mut plain_text);
        Ok(plain_text)
    }
}

fn calculate_mac(ciphertext: &[u8], kdf_key: &[u8; 32]) -> [u8; 32] {
    let ciphertext_len = ciphertext.len();
    let mut mac_bytes = vec![0u8; 16 + ciphertext_len];
    mac_bytes[..16].copy_from_slice(&kdf_key[16..]);
    mac_bytes[16..16 + ciphertext_len].copy_from_slice(ciphertext);
    let mut output = [0u8; 32];
    let mut hasher = tiny_keccak::Keccak::v256();
    hasher.update(&mac_bytes);
    hasher.finalize(&mut output);
    output
}

macro_rules! impl_deserialize_bytes {
    ($name: ident, $size: expr) => {
        pub fn $name<'de, D>(deserializer: D) -> Result<[u8; $size], D::Error>
        where
            D: Deserializer<'de>,
        {
            let s = String::deserialize(deserializer)?;
            let mut res = [0u8; $size];
            hex::decode_to_slice(s, &mut res).map_err(serde::de::Error::custom)?;
            Ok(res)
        }
    };
}
impl_deserialize_bytes!(deserialize_bytes_32, 32);
impl_deserialize_bytes!(deserialize_bytes_16, 16);

pub fn serialize_bytes<S>(data: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&hex::encode(data))
}

pub fn deserialize_bytes<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    hex::decode(s).map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::key::Generator;
    use serde_json::Value;

    struct Fixture {
        json_data: Value,
        password: Vec<u8>,
        key: [u8; 32],
    }

    // https://github.com/ethereum/wiki/wiki/Web3-Secret-Storage-Definition#pbkdf2-sha-256
    fn fixture() -> Fixture {
        let json_str = r#"
        {
            "cipher" : "aes-128-ctr",
            "cipherparams" : {
                "iv" : "6087dab2f9fdbbfaddc31a909735c1e6"
            },
            "ciphertext" : "5318b4d5bcd28de64ee5559e671353e16f075ecae9f99c7a79a38af5f869aa46",
            "kdf" : "pbkdf2",
            "kdfparams" : {
                "c" : 262144,
                "dklen" : 32,
                "prf" : "hmac-sha256",
                "salt" : "ae3cd4e7013836a3df6bd7241b12db061dbe2c6785853cce422d148a624ce0bd"
            },
            "mac" : "517ead924a9d0dc3124507e3393d175ce3ff7c1e96529c6c555ce9e51205e9b2"
        }"#;
        let mut key = [0u8; 32];
        hex::decode_to_slice(
            "7a28b5ba57c53603b0b07b56bba752f7784bf506fa95edc395f5cf6c7514fe9d",
            &mut key,
        )
        .unwrap();
        Fixture {
            json_data: serde_json::from_str(json_str).unwrap(),
            password: b"testpassword".to_vec(),
            key,
        }
    }

    #[test]
    fn test_decrypt() {
        let fixture = fixture();
        let crypto: Crypto = serde_json::from_value(fixture.json_data).unwrap();
        let key = crypto.decrypt_key(fixture.password.as_slice()).unwrap();
        assert_eq!(key, fixture.key);

        let result = crypto.decrypt_key("wrongpassword".as_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt() {
        let fixture = fixture();
        let mut salt = [0u8; 32];
        hex::decode_to_slice(
            fixture.json_data["kdfparams"]["salt"].as_str().unwrap(),
            &mut salt,
        )
        .unwrap();
        let kdf = Kdf::Pbkdf2(Pbkdf2::new_with_salt(salt));

        let mut iv = [0u8; 16];
        hex::decode_to_slice(
            fixture.json_data["cipherparams"]["iv"].as_str().unwrap(),
            &mut iv,
        )
        .unwrap();
        let cipher = Cipher::Aes128Ctr(Aes128CtrParams::new(iv));

        let password = "testpassword".as_bytes();

        let crypto = Crypto::encrypt_key_with_kdf_and_cipher(&fixture.key, password, kdf, cipher);
        assert_eq!(
            hex::encode(crypto.ciphertext),
            fixture.json_data["ciphertext"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(crypto.mac),
            fixture.json_data["mac"].as_str().unwrap()
        );
    }

    #[test]
    fn test_json() {
        let keypair = Generator::default().generate();
        let crypto: Crypto = Crypto::encrypt_key(keypair.secret().as_bytes(), &[]);
        let json_str = serde_json::to_string_pretty(&crypto).unwrap();

        let crypto2: Crypto = serde_json::from_str(&json_str).unwrap();

        assert_eq!(crypto, crypto2)
    }
}
