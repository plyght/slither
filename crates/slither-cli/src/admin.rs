use crate::AdminAction;

pub async fn run_admin(action: AdminAction, server: String, key: String) {
    let client = reqwest::Client::new();

    match action {
        AdminAction::Seeds => {
            let url = format!("{server}/admin/seeds");
            match client.get(&url).header("X-Api-Key", &key).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    match resp.text().await {
                        Ok(body) => {
                            if status.is_success() {
                                println!("{body}");
                            } else {
                                eprintln!("Error {status}: {body}");
                                std::process::exit(1);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to read response: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Request failed: {e}");
                    std::process::exit(1);
                }
            }
        }

        AdminAction::AddSeed {
            url: seed_url,
            depth,
            scope,
            priority,
            sitemap,
        } => {
            let url = format!("{server}/admin/seeds");
            let mut body = serde_json::json!({ "url": seed_url });
            if let Some(d) = depth {
                body["depth"] = serde_json::json!(d);
            }
            if let Some(s) = scope {
                body["scope"] = serde_json::json!(s);
            }
            if let Some(p) = priority {
                body["priority"] = serde_json::json!(p);
            }
            if sitemap {
                body["sitemap"] = serde_json::json!(true);
            }
            let body = body;
            match client
                .post(&url)
                .header("X-Api-Key", &key)
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    match resp.text().await {
                        Ok(body) => {
                            if status.is_success() {
                                println!("{body}");
                            } else {
                                eprintln!("Error {status}: {body}");
                                std::process::exit(1);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to read response: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Request failed: {e}");
                    std::process::exit(1);
                }
            }
        }

        AdminAction::RemoveSeed { url: seed_url } => {
            let url = format!("{server}/admin/seeds");
            let body = serde_json::json!({ "url": seed_url });
            match client
                .delete(&url)
                .header("X-Api-Key", &key)
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    match resp.text().await {
                        Ok(body) => {
                            if status.is_success() {
                                println!("{body}");
                            } else {
                                eprintln!("Error {status}: {body}");
                                std::process::exit(1);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to read response: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Request failed: {e}");
                    std::process::exit(1);
                }
            }
        }

        AdminAction::Crawl => {
            let url = format!("{server}/admin/crawl");
            match client
                .post(&url)
                .header("X-Api-Key", &key)
                .json(&serde_json::json!({}))
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    match resp.text().await {
                        Ok(body) => {
                            if status.is_success() {
                                println!("{body}");
                            } else {
                                eprintln!("Error {status}: {body}");
                                std::process::exit(1);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to read response: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Request failed: {e}");
                    std::process::exit(1);
                }
            }
        }

        AdminAction::Status => {
            let url = format!("{server}/admin/status");
            match client.get(&url).header("X-Api-Key", &key).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    match resp.text().await {
                        Ok(body) => {
                            if status.is_success() {
                                println!("{body}");
                            } else {
                                eprintln!("Error {status}: {body}");
                                std::process::exit(1);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to read response: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Request failed: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}
