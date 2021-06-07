pub enum KeyType {
    ED25519,
    SECP256K1,
}

impl KeyType {
    pub fn to_i32(&self) -> i32 {
        match self {
            &Self::ED25519 => {
                return 0;
            }
            _ => {
                return 1;
            }
        }
    }
    pub fn from_i32(index: i32) -> Self {
        if index == 0 {
            return Self::ED25519;
        }
        return Self::SECP256K1;
    }
    pub fn from_str(key_type: &str) -> Self {
        if key_type == "ed25519" {
            return Self::ED25519;
        }
        return Self::SECP256K1;
    }
}
