use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PasswordGenOptions {
    pub length: usize,
    pub uppercase: bool,
    pub lowercase: bool,
    pub numbers: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

impl Default for PasswordGenOptions {
    fn default() -> Self {
        Self {
            length: 20,
            uppercase: true,
            lowercase: true,
            numbers: true,
            symbols: true,
            exclude_ambiguous: false,
        }
    }
}

pub fn generate_custom_password(opts: PasswordGenOptions) -> Result<String, String> {
    if !(4..=128).contains(&opts.length) {
        return Err("密码长度必须在 4–128 位之间".to_string());
    }

    let mut lower = b"abcdefghijklmnopqrstuvwxyz".to_vec();
    let mut upper = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_vec();
    let mut digits = b"0123456789".to_vec();
    let mut symbols = b"!@#$%&*+-=?_".to_vec();

    if opts.exclude_ambiguous {
        // Remove 0, O, o, 1, l, I
        lower.retain(|&c| c != b'l' && c != b'o');
        upper.retain(|&c| c != b'I' && c != b'O');
        digits.retain(|&c| c != b'0' && c != b'1');
        symbols.retain(|&c| c != b'|');
    }

    let mut candidate_pools: Vec<&[u8]> = Vec::new();
    if opts.lowercase && !lower.is_empty() {
        candidate_pools.push(&lower);
    }
    if opts.uppercase && !upper.is_empty() {
        candidate_pools.push(&upper);
    }
    if opts.numbers && !digits.is_empty() {
        candidate_pools.push(&digits);
    }
    if opts.symbols && !symbols.is_empty() {
        candidate_pools.push(&symbols);
    }

    if candidate_pools.is_empty() {
        return Err("请至少勾选一种字符类型".to_string());
    }

    let mut rng = rand::thread_rng();
    let mut password = Vec::with_capacity(opts.length);

    // Guarantee at least one from each selected pool
    for pool in &candidate_pools {
        if password.len() < opts.length {
            password.push(*pool.choose(&mut rng).unwrap());
        }
    }

    let mut all_chars = Vec::new();
    for pool in &candidate_pools {
        all_chars.extend_from_slice(pool);
    }

    while password.len() < opts.length {
        password.push(*all_chars.choose(&mut rng).unwrap());
    }

    password.shuffle(&mut rng);
    String::from_utf8(password).map_err(|e| e.to_string())
}

pub fn generate_password(length: usize, symbols: bool) -> Result<String, String> {
    generate_custom_password(PasswordGenOptions {
        length,
        uppercase: true,
        lowercase: true,
        numbers: true,
        symbols,
        exclude_ambiguous: false,
    })
}
