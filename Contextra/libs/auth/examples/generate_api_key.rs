use auth::hash_api_key;
use uuid::Uuid;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key_id = format!("ctx_live_{}", Uuid::now_v7().simple());
    let secret = Uuid::now_v7().simple().to_string();

    let raw_key = format!("{}.{}", key_id, secret);
    let hash = hash_api_key(&secret)?;

    println!("KEY_ID={}", key_id);
    println!("RAW_KEY={}", raw_key);
    println!("HASH={}", hash);
    Ok(())
}
