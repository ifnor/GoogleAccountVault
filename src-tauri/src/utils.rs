use rand::seq::SliceRandom;

pub fn generate_password(length: usize, symbols: bool) -> Result<String, String> {
    if !(12..=128).contains(&length) {
        return Err("密码长度必须在 12–128 位之间".to_string());
    }

    let lowercase = b"abcdefghijklmnopqrstuvwxyz";
    let uppercase = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let digits = b"0123456789";
    let symbol_chars = b"!@#$%&*+-=?_";

    let mut rng = rand::thread_rng();
    let mut password = Vec::with_capacity(length);

    // Ensure at least one from each required pool
    password.push(*lowercase.choose(&mut rng).unwrap());
    password.push(*uppercase.choose(&mut rng).unwrap());
    password.push(*digits.choose(&mut rng).unwrap());
    if symbols {
        password.push(*symbol_chars.choose(&mut rng).unwrap());
    }

    let mut all_chars = Vec::new();
    all_chars.extend_from_slice(lowercase);
    all_chars.extend_from_slice(uppercase);
    all_chars.extend_from_slice(digits);
    if symbols {
        all_chars.extend_from_slice(symbol_chars);
    }

    while password.len() < length {
        password.push(*all_chars.choose(&mut rng).unwrap());
    }

    password.shuffle(&mut rng);
    String::from_utf8(password).map_err(|e| e.to_string())
}
