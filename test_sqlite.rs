use rustyclaw::storage::sqlite::SqliteStorage;

#[tokio::main]
async fn main() {
    let db_path = "test_direct.db";
    
    println!("Creating SQLite storage at: {}", db_path);
    
    match SqliteStorage::new(db_path).await {
        Ok(_) => println!("✓ SQLite storage created successfully!"),
        Err(e) => println!("✗ Failed to create storage: {}", e),
    }
}
