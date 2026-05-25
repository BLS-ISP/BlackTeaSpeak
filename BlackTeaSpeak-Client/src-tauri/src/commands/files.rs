#[tauri::command]
pub async fn upload_file(app: tauri::AppHandle, ip: String, port: u16, ftkey: String, size: u64, file_data: Vec<u8>, client_ft_id: u32) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    let host = if ip.is_empty() { "127.0.0.1".to_string() } else { ip };
    let url = format!("https://{}:{}/?transfer-key={}", host, port, ftkey);
    
    let res = client.post(&url)
        .body(file_data)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status().is_success() {
        Ok(())
    } else {
        Err(format!("Server returned status: {}", res.status()))
    }
}

#[tauri::command]
pub async fn download_file(app: tauri::AppHandle, ip: String, port: u16, ftkey: String, size: u64, client_ft_id: u32) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    let host = if ip.is_empty() { "127.0.0.1".to_string() } else { ip };
    let url = format!("https://{}:{}/?transfer-key={}", host, port, ftkey);
    
    let res = client.get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status().is_success() {
        let bytes = res.bytes().await.map_err(|e| e.to_string())?;
        Ok(bytes.to_vec())
    } else {
        Err(format!("Server returned status: {}", res.status()))
    }
}
